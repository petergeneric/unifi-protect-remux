using CommunityToolkit.Mvvm.ComponentModel;
using System.Collections.ObjectModel;

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

    [ObservableProperty]
    private FileStatus _status = FileStatus.Pending;

    [ObservableProperty]
    private string? _error;

    public ObservableCollection<string> OutputFiles { get; } = new();

    public QueuedFile(string path)
    {
        Path = path;
        FileName = System.IO.Path.GetFileName(path);
        OutputFiles.CollectionChanged += (_, _) => OnPropertyChanged(nameof(StatusLabel));
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
}
