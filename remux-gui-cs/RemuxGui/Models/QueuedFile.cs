using System;
using Avalonia.Media.Imaging;
using CommunityToolkit.Mvvm.ComponentModel;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;

namespace RemuxGui.Models;

public enum FileStatus
{
    Pending,
    Processing,
    Completed,
    Failed
}

public partial class QueuedFile : ObservableObject
{
    public string Path { get; }
    public string FileName { get; }
    public long? FileSize { get; }
    public string? FileSizeLabel { get; }
    public string? MacAddress { get; }
    public DateTimeOffset? FileTimestamp { get; }
    public string? FileTimestampLabel { get; }

    [ObservableProperty]
    private FileStatus _status = FileStatus.Pending;

    [ObservableProperty]
    private string? _error;

    [ObservableProperty]
    private int? _partitionCount;

    [ObservableProperty]
    private string? _cameraName;

    [ObservableProperty]
    private Bitmap? _thumbnail;

    public ObservableCollection<string> OutputFiles { get; } = new();

    public string? OutputSizeLabel
    {
        get
        {
            long total = 0;
            foreach (var file in OutputFiles)
            {
                try
                {
                    var info = new FileInfo(file);
                    if (info.Exists)
                        total += info.Length;
                }
                catch { }
            }
            return total > 0 ? FormatFileSize(total) : null;
        }
    }

    public QueuedFile(string path)
    {
        Path = path;
        FileName = System.IO.Path.GetFileName(path);
        MacAddress = ExtractMac(FileName);
        FileTimestamp = ExtractTimestamp(FileName);
        if (FileTimestamp is DateTimeOffset ts)
            FileTimestampLabel = ts.LocalDateTime.ToString("yyyy-MM-dd HH:mm:ss");
        OutputFiles.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(StatusLabel));
            OnPropertyChanged(nameof(OutputSizeLabel));
        };

        try
        {
            var info = new FileInfo(path);
            if (info.Exists)
            {
                FileSize = info.Length;
                FileSizeLabel = FormatFileSize(info.Length);
            }
        }
        catch
        {
            // Ignore errors reading file info
        }
    }

    public string StatusLabel => Status switch
    {
        FileStatus.Pending => "Pending",
        FileStatus.Processing => "Processing...",
        FileStatus.Completed when OutputFiles.Count == 1 => "Done (1 file)",
        FileStatus.Completed when OutputFiles.Count > 1 => $"Done ({OutputFiles.Count} files)",
        FileStatus.Completed => "Done",
        FileStatus.Failed => "Failed",
        _ => "Unknown"
    };

    partial void OnStatusChanged(FileStatus value)
    {
        OnPropertyChanged(nameof(StatusLabel));
    }

    public static string FormatFileSize(long bytes)
    {
        if (bytes < 1024)
            return $"{bytes} B";
        if (bytes < 1024 * 1024)
            return $"{bytes / 1024.0:F0} KB";
        if (bytes < 1024L * 1024 * 1024)
            return $"{bytes / (1024.0 * 1024):F0} MB";
        return $"{bytes / (1024.0 * 1024 * 1024):F1} GB";
    }

    private static string? ExtractMac(string fileName)
    {
        var underscoreIdx = fileName.IndexOf('_');
        if (underscoreIdx != 12)
            return null;

        var prefix = fileName[..12].ToUpperInvariant();
        if (prefix.All(c => (c >= '0' && c <= '9') || (c >= 'A' && c <= 'F')))
            return prefix;

        return null;
    }

    private static DateTimeOffset? ExtractTimestamp(string fileName)
    {
        // Strip extensions: .ubv or .ubv.gz
        var name = fileName;
        if (name.EndsWith(".gz", StringComparison.OrdinalIgnoreCase))
            name = name[..^3];
        if (name.EndsWith(".ubv", StringComparison.OrdinalIgnoreCase))
            name = name[..^4];

        var lastUnderscore = name.LastIndexOf('_');
        if (lastUnderscore < 0)
            return null;

        var segment = name[(lastUnderscore + 1)..];
        if (long.TryParse(segment, out var millis) && millis > 1_000_000_000_000L && millis < 10_000_000_000_000L)
            return DateTimeOffset.FromUnixTimeMilliseconds(millis);

        return null;
    }
}
