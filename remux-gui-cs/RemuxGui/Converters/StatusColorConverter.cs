using System;
using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;
using RemuxGui.Models;

namespace RemuxGui.Converters;

public class StatusColorConverter : IValueConverter
{
    private static readonly IBrush FailedBrush = new SolidColorBrush(Color.Parse("#C83C3C"));
    private static readonly IBrush ProcessingBrush = new SolidColorBrush(Color.Parse("#B48C14"));
    private static readonly IBrush CompletedBrush = new SolidColorBrush(Color.Parse("#28A03C"));
    private static readonly IBrush PendingBrush = new SolidColorBrush(Color.Parse("#6E6E78"));

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is FileStatus status)
        {
            return status switch
            {
                FileStatus.Failed => FailedBrush,
                FileStatus.Processing => ProcessingBrush,
                FileStatus.Completed => CompletedBrush,
                FileStatus.Pending => PendingBrush,
                _ => PendingBrush,
            };
        }
        return PendingBrush;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotSupportedException();
    }
}
