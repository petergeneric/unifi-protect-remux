using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Runtime.InteropServices;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using RemuxGui.Interop;

namespace RemuxGui.Views;

public partial class AboutView : UserControl
{
    public AboutView()
    {
        InitializeComponent();
        LoadVersionInfo();
        LoadLibraries();

        GithubLink.PointerPressed += OnGithubLinkClick;
        IconAttributionLink.Click += OnIconAttributionLinkClick;
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

    private void LoadLibraries()
    {
        var items = new List<LibraryItem>
        {
            new("Avalonia \u2014 cross-platform .NET UI framework (MIT)",
                "https://github.com/AvaloniaUI/Avalonia"),
            new("CommunityToolkit.Mvvm \u2014 MVVM toolkit (MIT)",
                "https://github.com/CommunityToolkit/dotnet"),
            new("FFmpeg \u2014 multimedia framework (LGPL/GPL)",
                "https://ffmpeg.org/"),
            new("MessageBox.Avalonia \u2014 message box dialogs (MIT)",
                "https://github.com/AvaloniaCommunity/MessageBox.Avalonia"),
        };

        try
        {
            var licenses = RemuxNative.GetLicenses();
            foreach (var entry in licenses)
            {
                var license = string.IsNullOrEmpty(entry.License) ? "unknown" : entry.License;
                var text = $"{entry.Name} {entry.Version} ({license})";
                var url = string.IsNullOrEmpty(entry.Repository) ? null : entry.Repository;
                items.Add(new LibraryItem(text, url));
            }
        }
        catch
        {
            // If FFI fails, we still show the static entries
        }

        items.Sort((a, b) => string.Compare(a.SortKey, b.SortKey, StringComparison.OrdinalIgnoreCase));
        LibrariesList.ItemsSource = items;
    }

    private static void OpenUrl(string url)
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

    private void OnGithubLinkClick(object? sender, PointerPressedEventArgs e)
    {
        OpenUrl("https://github.com/petergeneric/unifi-protect-remux");
    }

    private void OnIconAttributionLinkClick(object? sender, RoutedEventArgs e)
    {
        OpenUrl("https://thenounproject.com/icon/cctv-1925352/");
    }

    private void OnLibraryLinkClick(object? sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string url } && !string.IsNullOrEmpty(url))
            OpenUrl(url);
    }
}

public class LibraryItem
{
    public string DisplayText { get; }
    public string? Url { get; }
    public bool HasUrl => !string.IsNullOrEmpty(Url);
    public string SortKey { get; }

    public LibraryItem(string text, string? url = null)
    {
        DisplayText = $"\u2022 {text}";
        SortKey = text;
        Url = url;
    }
}
