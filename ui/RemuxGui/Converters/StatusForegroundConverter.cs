using System;
using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;
using RemuxGui.Models;

namespace RemuxGui.Converters;

public class StatusForegroundConverter : IValueConverter
{
    private static readonly IBrush WhiteBrush = new SolidColorBrush(Colors.White);

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is FileStatus)
        {
            return WhiteBrush;
        }
        return WhiteBrush;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotSupportedException();
    }
}
