using System;
using System.Globalization;
using System.Text.RegularExpressions;
using Avalonia.Data.Converters;

namespace RemuxGui.Converters;

public partial class FileNameConverter : IValueConverter
{
    public static readonly FileNameConverter Instance = new();

    // Matches _YYYY-MM-DDTHH.MM.SSZ.ext at the end of a filename
    [GeneratedRegex(@"_(\d{4}-\d{2}-\d{2}T[\d.]+Z)(\.\w+)$")]
    private static partial Regex DateSuffixRegex();

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is string path && !string.IsNullOrEmpty(path))
        {
            var filename = System.IO.Path.GetFileName(path);
            var match = DateSuffixRegex().Match(filename);
            if (match.Success)
            {
                // "2024-01-15T08.30.00Z" — replace dots with colons for readability, drop extension
                return match.Groups[1].Value.Replace('.', ':');
            }
            return System.IO.Path.GetFileNameWithoutExtension(filename);
        }
        return value;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotSupportedException();
    }
}
