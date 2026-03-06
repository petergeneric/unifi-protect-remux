using System;
using System.Collections.Generic;
using System.Linq;
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
        LicenseLink.Click += OnLicenseLinkClick;
    }

    private void LoadVersionInfo()
    {
        try
        {
            var info = RemuxNative.GetVersion();
            VersionText.Text = info.Version;

            if (!string.IsNullOrEmpty(info.GitCommit))
            {
                CommitText.Text = info.ShortCommit;
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
            CommitLabel.IsVisible = false;
            CommitText.IsVisible = false;
        }
    }

    private void LoadLibraries()
    {
        var items = new List<LibraryItem>
        {
            new("CCTV Camera icon by Vectors Market (CC BY 3.0)", "https://thenounproject.com/icon/cctv-1925352/"),
            new("Avalonia UI (MIT)",
                "https://github.com/AvaloniaUI/Avalonia"),
            new("CommunityToolkit.Mvvm (MIT)",
                "https://github.com/CommunityToolkit/dotnet"),
            new("FFmpeg multimedia framework (LGPL/GPL)",
                "https://ffmpeg.org/"),
            new("MessageBox.Avalonia (MIT)",
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

    private void OnGithubLinkClick(object? sender, PointerPressedEventArgs e)
    {
        PlatformHelper.OpenUrl("https://github.com/petergeneric/unifi-protect-remux");
    }

    private void OnLicenseLinkClick(object? sender, RoutedEventArgs e)
    {
        PlatformHelper.OpenUrl("https://www.gnu.org/licenses/agpl-3.0.html");
    }

    private void OnLibraryLinkClick(object? sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string url } && !string.IsNullOrEmpty(url))
            PlatformHelper.OpenUrl(url);
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
