using System;
using System.Globalization;
using Avalonia;
using Avalonia.Data.Converters;
using Avalonia.Media;

namespace RemuxGui.Converters;

public class ViewIndexConverter : IValueConverter
{
    private static readonly IBrush ActiveBgBrush = new SolidColorBrush(Color.Parse("#20FFFFFF"));
    private static readonly IBrush TransparentBrush = new SolidColorBrush(Colors.Transparent);
    private static readonly IBrush ActiveFgBrush = new SolidColorBrush(Color.Parse("#FFFFFF"));
    private static readonly IBrush InactiveFgBrush = new SolidColorBrush(Color.Parse("#A8ADBC"));

    /// <summary>
    /// "bg" returns background brush, "fg" returns foreground brush (UnsetValue when inactive).
    /// Omit for bool/double returns.
    /// </summary>
    public string Mode { get; set; } = "";

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is int current && parameter is string paramStr && int.TryParse(paramStr, out var target))
        {
            var isMatch = current == target;

            // Return double when used for Opacity binding
            if (targetType == typeof(double))
                return isMatch ? 1.0 : 0.0;

            if (targetType == typeof(IBrush) || targetType == typeof(Brush))
            {
                if (Mode == "fg")
                    return isMatch ? ActiveFgBrush : InactiveFgBrush;

                // Default: background mode
                return isMatch ? ActiveBgBrush : TransparentBrush;
            }

            return isMatch;
        }
        return AvaloniaProperty.UnsetValue;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotSupportedException();
    }
}
