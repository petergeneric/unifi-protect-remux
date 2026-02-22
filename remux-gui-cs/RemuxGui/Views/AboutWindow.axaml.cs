using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using RemuxGui.Interop;

namespace RemuxGui.Views;

public partial class AboutWindow : Window
{
    public AboutWindow()
    {
        InitializeComponent();
        LoadVersionInfo();

        GithubLink.PointerPressed += OnGithubLinkClick;
    }

    private void LoadVersionInfo()
    {
        try
        {
            var info = RemuxNative.GetVersion();
            VersionText.Text = $"v{info.Version}";

            if (!string.IsNullOrEmpty(info.ReleaseVersion))
            {
                ReleaseText.Text = info.ReleaseVersion;
            }
            else
            {
                ReleaseLabel.IsVisible = false;
                ReleaseText.IsVisible = false;
            }

            if (!string.IsNullOrEmpty(info.GitCommit))
            {
                CommitText.Text = info.GitCommit.Length > 10
                    ? info.GitCommit[..10]
                    : info.GitCommit;
            }
            else
            {
                CommitLabel.IsVisible = false;
                CommitText.IsVisible = false;
            }
        }
        catch
        {
            VersionText.Text = "unknown";
            ReleaseLabel.IsVisible = false;
            ReleaseText.IsVisible = false;
            CommitLabel.IsVisible = false;
            CommitText.IsVisible = false;
        }
    }

    private void OnGithubLinkClick(object? sender, PointerPressedEventArgs e)
    {
        var url = "https://github.com/petergeneric/unifi-protect-remux";
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

    private void OnClose(object? sender, RoutedEventArgs e)
    {
        Close();
    }
}
