using CommunityToolkit.Mvvm.ComponentModel;

namespace RemuxGui.Models;

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
