using System;
using System.Globalization;
using Avalonia;
using Avalonia.Data.Converters;
using Avalonia.Media;

namespace RemuxGui.Converters;

public class LogFilterActiveConverter : IValueConverter
{
    public string Mode { get; set; } = "bg";

    private static readonly IBrush AccentBg = new SolidColorBrush(Color.Parse("#6C8CFF"));
    private static readonly IBrush ActiveFg = new SolidColorBrush(Colors.White);

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is string currentFilter && parameter is string pillLevel)
        {
            var isActive = string.Equals(currentFilter, pillLevel, StringComparison.OrdinalIgnoreCase);

            if (!isActive)
                return AvaloniaProperty.UnsetValue;

            return Mode == "fg" ? ActiveFg : AccentBg;
        }

        return AvaloniaProperty.UnsetValue;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotSupportedException();
    }
}
