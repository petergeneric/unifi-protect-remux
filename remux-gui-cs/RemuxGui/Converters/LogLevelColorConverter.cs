using System;
using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;

namespace RemuxGui.Converters;

public class LogLevelColorConverter : IValueConverter
{
    private static readonly IBrush ErrorBrush = new SolidColorBrush(Color.FromRgb(240, 90, 90));
    private static readonly IBrush WarnBrush = new SolidColorBrush(Color.FromRgb(220, 180, 50));
    private static readonly IBrush InfoBrush = new SolidColorBrush(Color.FromRgb(170, 170, 180));

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is string level)
        {
            return level.ToLowerInvariant() switch
            {
                "error" => ErrorBrush,
                "warn" => WarnBrush,
                _ => InfoBrush,
            };
        }
        return InfoBrush;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotSupportedException();
    }
}
