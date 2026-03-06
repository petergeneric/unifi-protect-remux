using System.Diagnostics;
using System.Runtime.InteropServices;

namespace RemuxGui.Interop;

public static class PlatformHelper
{
    public static void OpenUrl(string url)
    {
        try
        {
            if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
                Process.Start(new ProcessStartInfo(url) { UseShellExecute = true });
            else if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX))
                Process.Start("open", url);
            else
                Process.Start("xdg-open", url);
        }
        catch
        {
            // Silently ignore if we can't open the browser
        }
    }

    public static void RevealInFileBrowser(string path)
    {
        try
        {
            if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
                Process.Start("explorer.exe", $"/select,\"{path}\"");
            else if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX))
                Process.Start("open", $"-R \"{path}\"");
            else
                Process.Start("xdg-open", System.IO.Path.GetDirectoryName(path) ?? path);
        }
        catch
        {
            // Silently ignore if we can't open the file browser
        }
    }
}
