using System;
using CommunityToolkit.Mvvm.ComponentModel;

namespace RemuxGui.Models;

public partial class LogEntry : ObservableObject
{
    public string Level { get; }
    public string Message { get; }
    public DateTime Timestamp { get; }
    public string TimestampLabel => Timestamp.ToString("HH:mm:ss");
    public int? FileIndex { get; }

    public LogEntry(string level, string message, int? fileIndex = null)
    {
        Level = level;
        Message = message;
        Timestamp = DateTime.Now;
        FileIndex = fileIndex;
    }
}
