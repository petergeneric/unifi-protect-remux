using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using RemuxGui.Interop;
using RemuxGui.Models;

namespace RemuxGui.ViewModels;

public partial class LogEntry : ObservableObject
{
    public string Level { get; }
    public string Message { get; }

    public LogEntry(string level, string message)
    {
        Level = level;
        Message = message;
    }
}

public partial class MainViewModel : ViewModelBase
{
    public ObservableCollection<QueuedFile> Files { get; } = new();
    public ObservableCollection<LogEntry> LogLines { get; } = new();
    public ObservableCollection<string> OutputFiles { get; } = new();

    [ObservableProperty]
    private bool _isProcessing;

    [ObservableProperty]
    private bool _isDiagnosticsProcessing;

    [ObservableProperty]
    private bool _showSettings;

    // Settings
    [ObservableProperty]
    private bool _withAudio = true;

    [ObservableProperty]
    private bool _withVideo = true;

    [ObservableProperty]
    private decimal _forceRate;

    [ObservableProperty]
    private bool _fastStart;

    [ObservableProperty]
    private string _outputFolder = "SRC-FOLDER";

    [ObservableProperty]
    private bool _mp4Output = true;

    [ObservableProperty]
    private decimal _videoTrack;

    public bool IsBusy => IsProcessing || IsDiagnosticsProcessing;
    public bool CanStart => !IsBusy && Files.Count > 0;

    partial void OnIsProcessingChanged(bool value)
    {
        OnPropertyChanged(nameof(IsBusy));
        OnPropertyChanged(nameof(CanStart));
        StartCommand.NotifyCanExecuteChanged();
        DiagnosticsCommand.NotifyCanExecuteChanged();
        ClearCommand.NotifyCanExecuteChanged();
    }

    partial void OnIsDiagnosticsProcessingChanged(bool value)
    {
        OnPropertyChanged(nameof(IsBusy));
        OnPropertyChanged(nameof(CanStart));
        StartCommand.NotifyCanExecuteChanged();
        DiagnosticsCommand.NotifyCanExecuteChanged();
        ClearCommand.NotifyCanExecuteChanged();
    }

    public MainViewModel()
    {
        Files.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(CanStart));
            StartCommand.NotifyCanExecuteChanged();
            DiagnosticsCommand.NotifyCanExecuteChanged();
        };
    }

    /// <summary>
    /// Add files to the queue. Returns a list of paths that are low-resolution
    /// recordings and need user confirmation before being added.
    /// </summary>
    public List<string> AddFiles(IEnumerable<string> paths)
    {
        var warnedPaths = new List<string>();

        foreach (var path in paths)
        {
            var lower = path.ToLowerInvariant();
            if (!lower.EndsWith(".ubv") && !lower.EndsWith(".ubv.gz"))
                continue;

            if (Files.Any(f => f.Path == path))
                continue;

            if (lower.Contains("_2_rotating_") || lower.Contains("_timelapse_"))
            {
                warnedPaths.Add(path);
            }
            else
            {
                Files.Add(new QueuedFile(path));
            }
        }

        return warnedPaths;
    }

    /// <summary>
    /// Add previously-warned paths after user confirmation.
    /// </summary>
    public void AddWarnedFiles(IEnumerable<string> paths)
    {
        foreach (var path in paths)
        {
            Files.Add(new QueuedFile(path));
        }
    }

    private RemuxConfig BuildConfig()
    {
        return new RemuxConfig
        {
            WithAudio = WithAudio,
            WithVideo = WithVideo,
            ForceRate = (uint)Math.Max(0, ForceRate),
            FastStart = FastStart,
            OutputFolder = OutputFolder,
            Mp4 = Mp4Output,
            VideoTrack = (ushort)Math.Clamp(VideoTrack, 0, 65535),
        };
    }

    [RelayCommand(CanExecute = nameof(CanStart))]
    private async Task Start()
    {
        if (IsBusy || Files.Count == 0)
            return;

        LogLines.Clear();
        OutputFiles.Clear();

        foreach (var f in Files)
        {
            f.Status = FileStatus.Pending;
            f.OutputFiles.Clear();
            f.Error = null;
        }

        IsProcessing = true;
        var config = BuildConfig();
        var filePaths = Files.Select(f => f.Path).ToList();

        await Task.Run(() =>
        {
            RemuxNative.Init();

            for (int i = 0; i < filePaths.Count; i++)
            {
                var fileIndex = i;
                var path = filePaths[i];

                // Pin the callback delegate to prevent GC collection during native call
                ProgressCallback callback = (jsonPtr, idx) =>
                {
                    if (jsonPtr == IntPtr.Zero) return;
                    var json = Marshal.PtrToStringUTF8(jsonPtr);
                    if (json == null) return;

                    try
                    {
                        var evt = JsonSerializer.Deserialize(json, AppJsonContext.Default.ProgressEvent);
                        if (evt != null)
                        {
                            Dispatcher.UIThread.Post(() => HandleProgressEvent(idx, evt));
                        }
                    }
                    catch
                    {
                        // Ignore deserialization errors in callbacks
                    }
                };

                // Pin delegate so GC doesn't collect it during native call
                var gcHandle = GCHandle.Alloc(callback);
                try
                {
                    var (resultJson, error) = RemuxNative.ProcessFile(path, config, callback, fileIndex);

                    if (error != null)
                    {
                        Dispatcher.UIThread.Post(() =>
                        {
                            LogLines.Add(new LogEntry("error", $"Error processing {path}: {error}"));
                            if (fileIndex < Files.Count)
                            {
                                Files[fileIndex].Status = FileStatus.Failed;
                                Files[fileIndex].Error = error;
                            }
                        });
                    }
                }
                finally
                {
                    gcHandle.Free();
                }
            }
        });

        IsProcessing = false;
    }

    [RelayCommand(CanExecute = nameof(CanStart))]
    private async Task Diagnostics()
    {
        if (IsBusy || Files.Count == 0)
            return;

        LogLines.Clear();
        OutputFiles.Clear();

        foreach (var f in Files)
        {
            f.Status = FileStatus.Pending;
            f.OutputFiles.Clear();
            f.Error = null;
        }

        IsDiagnosticsProcessing = true;
        var filePaths = Files.Select(f => f.Path).ToList();

        await Task.Run(() =>
        {
            for (int i = 0; i < filePaths.Count; i++)
            {
                var fileIndex = i;
                var path = filePaths[i];

                Dispatcher.UIThread.Post(() =>
                {
                    if (fileIndex < Files.Count)
                        Files[fileIndex].Status = FileStatus.Processing;
                    LogLines.Add(new LogEntry("info", $"Producing diagnostics for file {fileIndex + 1}..."));
                });

                var (outputPath, error) = RemuxNative.ProduceDiagnostics(path);

                Dispatcher.UIThread.Post(() =>
                {
                    if (outputPath != null)
                    {
                        if (fileIndex < Files.Count)
                        {
                            Files[fileIndex].Status = FileStatus.Completed;
                            Files[fileIndex].OutputFiles.Add(outputPath);
                        }
                        OutputFiles.Add(outputPath);
                    }
                    else
                    {
                        if (fileIndex < Files.Count)
                        {
                            Files[fileIndex].Status = FileStatus.Failed;
                            Files[fileIndex].Error = error;
                        }
                        LogLines.Add(new LogEntry("error", error ?? "Unknown error"));
                    }
                });
            }
        });

        IsDiagnosticsProcessing = false;
    }

    [RelayCommand]
    private void Clear()
    {
        if (IsBusy) return;
        Files.Clear();
        LogLines.Clear();
        OutputFiles.Clear();
    }

    [RelayCommand]
    private void ToggleSettings()
    {
        ShowSettings = !ShowSettings;
    }

    private void HandleProgressEvent(int fileIndex, ProgressEvent evt)
    {
        switch (evt.Type)
        {
            case "log":
                LogLines.Add(new LogEntry(evt.Level ?? "info", evt.Message ?? ""));
                break;

            case "file_started":
                if (fileIndex < Files.Count)
                    Files[fileIndex].Status = FileStatus.Processing;
                break;

            case "partitions_found":
                LogLines.Add(new LogEntry("info", $"Found {evt.Count} partition(s)"));
                break;

            case "partition_started":
                LogLines.Add(new LogEntry("info", $"Processing partition {(evt.Index ?? 0) + 1}/{evt.Total}"));
                break;

            case "output_generated":
                if (evt.Path != null)
                {
                    if (fileIndex < Files.Count)
                        Files[fileIndex].OutputFiles.Add(evt.Path);
                    OutputFiles.Add(evt.Path);
                }
                break;

            case "partition_error":
                LogLines.Add(new LogEntry("error", $"Partition #{evt.Index}: {evt.Error}"));
                break;

            case "file_completed":
                if (fileIndex < Files.Count)
                {
                    if (evt.Errors == null || evt.Errors.Length == 0)
                    {
                        Files[fileIndex].Status = FileStatus.Completed;
                    }
                    else
                    {
                        Files[fileIndex].Status = FileStatus.Failed;
                        Files[fileIndex].Error = string.Join("; ", evt.Errors);
                    }
                }
                break;
        }
    }
}
