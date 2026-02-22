using System;
using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;

namespace RemuxGui.Converters;

public class ViewIndexConverter : IValueConverter
{
    private static readonly IBrush ActiveBrush = new SolidColorBrush(Color.Parse("#266C8CFF"));
    private static readonly IBrush TransparentBrush = new SolidColorBrush(Colors.Transparent);

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is int current && parameter is string paramStr && int.TryParse(paramStr, out var target))
        {
            var isMatch = current == target;

            // Return brush when used for Background binding
            if (targetType == typeof(IBrush) || targetType == typeof(Brush))
                return isMatch ? ActiveBrush : TransparentBrush;

            return isMatch;
        }
        return false;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotSupportedException();
    }
}
