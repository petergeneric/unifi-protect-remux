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

    [ObservableProperty]
    private FileStatus _status = FileStatus.Pending;

    [ObservableProperty]
    private string? _error;

    [ObservableProperty]
    private int? _partitionCount;

    [ObservableProperty]
    private string? _cameraName;

    public ObservableCollection<string> OutputFiles { get; } = new();

    public QueuedFile(string path)
    {
        Path = path;
        FileName = System.IO.Path.GetFileName(path);
        MacAddress = ExtractMac(FileName);
        OutputFiles.CollectionChanged += (_, _) => OnPropertyChanged(nameof(StatusLabel));

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
}
