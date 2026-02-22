using System;
using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;
using RemuxGui.Models;

namespace RemuxGui.Converters;

public class StatusColorConverter : IValueConverter
{
    private static readonly IBrush FailedBrush = new SolidColorBrush(Color.FromRgb(200, 60, 60));
    private static readonly IBrush ProcessingBrush = new SolidColorBrush(Color.FromRgb(180, 140, 20));
    private static readonly IBrush CompletedBrush = new SolidColorBrush(Color.FromRgb(40, 160, 60));
    private static readonly IBrush PendingBrush = new SolidColorBrush(Color.FromRgb(110, 110, 120));

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
