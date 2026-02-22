using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.ComponentModel;
using System.IO;
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

public partial class MainViewModel : ViewModelBase
{
    public ObservableCollection<QueuedFile> Files { get; } = new();
    public ObservableCollection<LogEntry> LogLines { get; } = new();
    public ObservableCollection<string> OutputFiles { get; } = new();
    public ObservableCollection<LogEntry> FilteredLogLines { get; } = new();
    public ObservableCollection<CameraEntry> Cameras { get; } = new();

    [ObservableProperty]
    private bool _isProcessing;

    [ObservableProperty]
    private bool _isDiagnosticsProcessing;

    // Navigation
    [ObservableProperty]
    private int _currentView;

    [ObservableProperty]
    private QueuedFile? _selectedFile;

    // Log filtering
    [ObservableProperty]
    private string _logFilterLevel = "All";

    [ObservableProperty]
    private string _logSearchText = "";

    [ObservableProperty]
    private int? _logFileFilter;

    [ObservableProperty]
    private int _infoCount;

    [ObservableProperty]
    private int _warnCount;

    [ObservableProperty]
    private int _errorCount;

    public string? LogFileFilterLabel => LogFileFilter is int idx && idx < Files.Count
        ? Files[idx].FileName
        : null;

    // Version
    public string VersionString { get; private set; } = "";

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
    private string _outputFolder = RemuxConfig.DefaultOutputFolder;

    [ObservableProperty]
    private bool _mp4Output = true;

    [ObservableProperty]
    private decimal _videoTrack;

    private CancellationTokenSource? _cts;

    public bool IsBusy => IsProcessing || IsDiagnosticsProcessing;
    public bool CanStart => !IsBusy && Files.Count > 0;
    public bool CanConvertFile => !IsBusy && SelectedFile != null;

    partial void OnIsProcessingChanged(bool value)
    {
        OnPropertyChanged(nameof(IsBusy));
        OnPropertyChanged(nameof(CanStart));
        OnPropertyChanged(nameof(CanConvertFile));
        StartCommand.NotifyCanExecuteChanged();
        DiagnosticsCommand.NotifyCanExecuteChanged();
        ConvertFileCommand.NotifyCanExecuteChanged();
    }

    partial void OnIsDiagnosticsProcessingChanged(bool value)
    {
        OnPropertyChanged(nameof(IsBusy));
        OnPropertyChanged(nameof(CanStart));
        OnPropertyChanged(nameof(CanConvertFile));
        StartCommand.NotifyCanExecuteChanged();
        DiagnosticsCommand.NotifyCanExecuteChanged();
        ConvertFileCommand.NotifyCanExecuteChanged();
    }

    partial void OnSelectedFileChanged(QueuedFile? value)
    {
        OnPropertyChanged(nameof(CanConvertFile));
        ConvertFileCommand.NotifyCanExecuteChanged();
    }

    partial void OnLogFilterLevelChanged(string value)
    {
        RebuildFilteredLogLines();
    }

    partial void OnLogSearchTextChanged(string value)
    {
        RebuildFilteredLogLines();
    }

    partial void OnLogFileFilterChanged(int? value)
    {
        OnPropertyChanged(nameof(LogFileFilterLabel));
        RebuildFilteredLogLines();
    }

    public MainViewModel()
    {
        Files.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(CanStart));
            StartCommand.NotifyCanExecuteChanged();
            DiagnosticsCommand.NotifyCanExecuteChanged();
        };

        LogLines.CollectionChanged += OnLogLinesChanged;
        Cameras.CollectionChanged += (_, _) => RefreshAllCameraNames();

        LoadVersionString();
        LoadCameras();
    }

    private void LoadVersionString()
    {
        try
        {
            var info = RemuxNative.GetVersion();
            var commit = info.GitCommit;
            if (commit.Length > 7)
                commit = commit[..7];

            if (!string.IsNullOrEmpty(commit))
                VersionString = $"v{info.Version} \u00b7 {commit}";
            else
                VersionString = $"v{info.Version}";
        }
        catch
        {
            VersionString = "";
        }
    }

    private void OnLogLinesChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        UpdateLogCounts();
        RebuildFilteredLogLines();
    }

    private void UpdateLogCounts()
    {
        int info = 0, warn = 0, error = 0;
        foreach (var entry in LogLines)
        {
            switch (entry.Level.ToLowerInvariant())
            {
                case "error": error++; break;
                case "warn": warn++; break;
                default: info++; break;
            }
        }
        InfoCount = info;
        WarnCount = warn;
        ErrorCount = error;
    }

    private void RebuildFilteredLogLines()
    {
        FilteredLogLines.Clear();
        var filterLevel = LogFilterLevel.ToLowerInvariant();
        var searchText = LogSearchText?.Trim() ?? "";
        var fileFilter = LogFileFilter;

        foreach (var entry in LogLines)
        {
            if (fileFilter != null && entry.FileIndex != fileFilter)
                continue;

            if (filterLevel != "all" && !entry.Level.Equals(filterLevel, StringComparison.OrdinalIgnoreCase))
                continue;

            if (searchText.Length > 0 &&
                !entry.Message.Contains(searchText, StringComparison.OrdinalIgnoreCase))
                continue;

            FilteredLogLines.Add(entry);
        }
    }

    [RelayCommand]
    private void SetView(string idx)
    {
        if (int.TryParse(idx, out var view))
            CurrentView = view;
    }

    [RelayCommand]
    private void SetLogFilter(string level)
    {
        LogFilterLevel = level;
    }

    [RelayCommand]
    private void ClearLog()
    {
        LogLines.Clear();
        FilteredLogLines.Clear();
    }

    [RelayCommand]
    private void ViewFileLog()
    {
        if (SelectedFile == null) return;
        var idx = Files.IndexOf(SelectedFile);
        if (idx < 0) return;
        LogFileFilter = idx;
        CurrentView = 2;
    }

    [RelayCommand]
    private void ClearLogFileFilter()
    {
        LogFileFilter = null;
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
                var qf = new QueuedFile(path);
                EnsureCameraEntry(qf.MacAddress);
                qf.CameraName = LookupCameraName(qf.MacAddress);
                Files.Add(qf);
            }
        }

        SelectedFile ??= Files.FirstOrDefault();
        return warnedPaths;
    }

    /// <summary>
    /// Add previously-warned paths after user confirmation.
    /// </summary>
    public void AddWarnedFiles(IEnumerable<string> paths)
    {
        foreach (var path in paths)
        {
            var qf = new QueuedFile(path);
            EnsureCameraEntry(qf.MacAddress);
            qf.CameraName = LookupCameraName(qf.MacAddress);
            Files.Add(qf);
        }

        SelectedFile ??= Files.FirstOrDefault();
    }

    private static string? SanitizeBaseName(string? name)
    {
        if (string.IsNullOrWhiteSpace(name))
            return null;

        var invalid = System.IO.Path.GetInvalidFileNameChars();
        var sanitized = new System.Text.StringBuilder(name.Length);
        foreach (var c in name)
        {
            if (Array.IndexOf(invalid, c) < 0)
                sanitized.Append(c);
        }

        var result = sanitized.ToString().Trim();
        return result.Length > 0 ? result : null;
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

    private bool TryBeginProcessing()
    {
        if (IsBusy || Files.Count == 0)
            return false;

        LogLines.Clear();
        OutputFiles.Clear();

        foreach (var f in Files)
        {
            f.Status = FileStatus.Pending;
            f.OutputFiles.Clear();
            f.Error = null;
        }

        return true;
    }

    [RelayCommand(CanExecute = nameof(CanStart))]
    private async Task Start()
    {
        if (!TryBeginProcessing())
            return;

        _cts = new CancellationTokenSource();
        var token = _cts.Token;

        IsProcessing = true;
        var config = BuildConfig();
        var filePaths = Files.Select(f => f.Path).ToList();
        var baseNames = Files.Select(f => SanitizeBaseName(f.CameraName)).ToList();

        await Task.Run(() =>
        {
            RemuxNative.Init();

            for (int i = 0; i < filePaths.Count; i++)
            {
                if (token.IsCancellationRequested)
                    break;

                ProcessSingleFile(filePaths[i], config, i, baseNames[i]);
            }
        });

        _cts?.Dispose();
        _cts = null;
        IsProcessing = false;
    }

    [RelayCommand(CanExecute = nameof(CanConvertFile))]
    private async Task ConvertFile()
    {
        if (IsBusy || SelectedFile == null)
            return;

        var fileIndex = Files.IndexOf(SelectedFile);
        if (fileIndex < 0)
            return;

        var path = SelectedFile.Path;
        var baseName = SanitizeBaseName(SelectedFile.CameraName);

        SelectedFile.Status = FileStatus.Pending;
        SelectedFile.OutputFiles.Clear();
        SelectedFile.Error = null;

        LogLines.Clear();
        OutputFiles.Clear();

        _cts = new CancellationTokenSource();
        IsProcessing = true;
        var config = BuildConfig();

        await Task.Run(() =>
        {
            RemuxNative.Init();
            ProcessSingleFile(path, config, fileIndex, baseName);
        });

        _cts?.Dispose();
        _cts = null;
        IsProcessing = false;
    }

    private void ProcessSingleFile(string path, RemuxConfig config, int fileIndex, string? baseName = null)
    {
        config.BaseName = baseName;
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

        var gcHandle = GCHandle.Alloc(callback);
        try
        {
            var (resultJson, error) = RemuxNative.ProcessFile(path, config, callback, fileIndex);

            if (error != null)
            {
                Dispatcher.UIThread.Post(() =>
                {
                    LogLines.Add(new LogEntry("error", $"Error processing {path}: {error}", fileIndex));
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

    [RelayCommand(CanExecute = nameof(CanStart))]
    private async Task Diagnostics()
    {
        if (!TryBeginProcessing())
            return;

        _cts = new CancellationTokenSource();
        var token = _cts.Token;

        IsDiagnosticsProcessing = true;
        var filePaths = Files.Select(f => f.Path).ToList();

        await Task.Run(() =>
        {
            RemuxNative.Init();

            for (int i = 0; i < filePaths.Count; i++)
            {
                if (token.IsCancellationRequested)
                    break;

                var fileIndex = i;
                var path = filePaths[i];

                Dispatcher.UIThread.Post(() =>
                {
                    if (fileIndex < Files.Count)
                        Files[fileIndex].Status = FileStatus.Processing;
                    LogLines.Add(new LogEntry("info", $"Producing diagnostics for file {fileIndex + 1}...", fileIndex));
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
                        LogLines.Add(new LogEntry("error", error ?? "Unknown error", fileIndex));
                    }
                });
            }
        });

        _cts?.Dispose();
        _cts = null;
        IsDiagnosticsProcessing = false;
    }

    [RelayCommand]
    private void Cancel()
    {
        _cts?.Cancel();
    }

    [RelayCommand]
    private void RemoveFile(QueuedFile? file)
    {
        if (file == null || IsBusy) return;

        var idx = Files.IndexOf(file);
        if (idx < 0) return;

        Files.RemoveAt(idx);

        if (SelectedFile == file)
            SelectedFile = Files.Count > 0 ? Files[Math.Min(idx, Files.Count - 1)] : null;
    }

    // --- Camera management ---

    private static string CamerasFilePath => System.IO.Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "RemuxGui", "cameras.json");

    [RelayCommand]
    private void RemoveCamera(CameraEntry? entry)
    {
        if (entry == null) return;
        entry.PropertyChanged -= OnCameraEntryPropertyChanged;
        Cameras.Remove(entry);
        SaveCameras();
    }

    private void EnsureCameraEntry(string? mac)
    {
        if (string.IsNullOrEmpty(mac)) return;
        if (Cameras.Any(c => string.Equals(c.MacAddress, mac, StringComparison.OrdinalIgnoreCase)))
            return;

        var entry = new CameraEntry(mac, "");
        entry.PropertyChanged += OnCameraEntryPropertyChanged;
        Cameras.Add(entry);
    }

    private void OnCameraEntryPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(CameraEntry.FriendlyName))
        {
            RefreshAllCameraNames();
            SaveCameras();
        }
    }

    private string? LookupCameraName(string? mac)
    {
        if (string.IsNullOrEmpty(mac)) return null;
        foreach (var cam in Cameras)
        {
            if (string.Equals(cam.MacAddress, mac, StringComparison.OrdinalIgnoreCase)
                && !string.IsNullOrWhiteSpace(cam.FriendlyName))
                return cam.FriendlyName;
        }
        return null;
    }

    private void RefreshAllCameraNames()
    {
        foreach (var file in Files)
            file.CameraName = LookupCameraName(file.MacAddress);
    }

    private void LoadCameras()
    {
        try
        {
            var path = CamerasFilePath;
            if (!File.Exists(path)) return;

            var json = File.ReadAllText(path);
            var data = JsonSerializer.Deserialize(json, AppJsonContext.Default.CameraData);
            if (data?.Cameras == null) return;

            foreach (var dto in data.Cameras)
            {
                var entry = new CameraEntry(dto.Mac, dto.Name);
                entry.PropertyChanged += OnCameraEntryPropertyChanged;
                Cameras.Add(entry);
            }
        }
        catch
        {
            // Ignore errors loading cameras
        }
    }

    private void SaveCameras()
    {
        try
        {
            var data = new CameraData();
            foreach (var cam in Cameras.Where(c => !string.IsNullOrWhiteSpace(c.FriendlyName)))
                data.Cameras.Add(new CameraDataEntry { Mac = cam.MacAddress, Name = cam.FriendlyName });

            var dir = System.IO.Path.GetDirectoryName(CamerasFilePath);
            if (dir != null)
                Directory.CreateDirectory(dir);

            var json = JsonSerializer.Serialize(data, AppJsonContext.Default.CameraData);
            File.WriteAllText(CamerasFilePath, json);
        }
        catch
        {
            // Ignore errors saving cameras
        }
    }

    private void ExtractThumbnailAsync(QueuedFile qf, string mp4Path)
    {
        try
        {
            var thumbPath = System.IO.Path.Combine(
                System.IO.Path.GetTempPath(),
                $"remuxgui_{Guid.NewGuid():N}.jpg");

            var error = RemuxNative.ExtractThumbnail(mp4Path, thumbPath);
            if (error != null) return;

            Dispatcher.UIThread.Post(() =>
            {
                try
                {
                    qf.Thumbnail = new Avalonia.Media.Imaging.Bitmap(thumbPath);
                }
                catch { }
                finally
                {
                    try { File.Delete(thumbPath); } catch { }
                }
            });
        }
        catch
        {
            // Thumbnail extraction is best-effort
        }
    }

    private void HandleProgressEvent(int fileIndex, ProgressEvent evt)
    {
        switch (evt.Type)
        {
            case "log":
                LogLines.Add(new LogEntry(evt.Level ?? "info", evt.Message ?? "", fileIndex));
                break;

            case "file_started":
                if (fileIndex < Files.Count)
                    Files[fileIndex].Status = FileStatus.Processing;
                break;

            case "partitions_found":
                if (fileIndex < Files.Count)
                    Files[fileIndex].PartitionCount = evt.Count;
                LogLines.Add(new LogEntry("info", $"Found {evt.Count} partition(s)", fileIndex));
                break;

            case "partition_started":
                LogLines.Add(new LogEntry("info", $"Processing partition {(evt.Index ?? 0) + 1}/{evt.Total}", fileIndex));
                break;

            case "output_generated":
                if (evt.Path != null)
                {
                    if (fileIndex < Files.Count)
                    {
                        var qf = Files[fileIndex];
                        qf.OutputFiles.Add(evt.Path);

                        // Extract thumbnail from first MP4 output
                        if (qf.Thumbnail == null &&
                            evt.Path.EndsWith(".mp4", StringComparison.OrdinalIgnoreCase))
                        {
                            var mp4Path = evt.Path;
                            _ = Task.Run(() => ExtractThumbnailAsync(qf, mp4Path));
                        }
                    }
                    OutputFiles.Add(evt.Path);
                }
                break;

            case "partition_error":
                LogLines.Add(new LogEntry("error", $"Partition #{evt.Index}: {evt.Error}", fileIndex));
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
