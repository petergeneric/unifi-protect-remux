using System;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using RemuxGui.Interop;

namespace RemuxGui.Views;

public partial class AboutWindow : Window
{

    public static async Task ShowAbout(Window owner)
    {
        var about = new AboutWindow();
        await about.ShowDialog(owner);
    }

    private void OnCloseWindow(object? sender, EventArgs e)
    {
        Close();
    }

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

    private void OnGithubLinkClick(object? sender, PointerPressedEventArgs e)
    {
        PlatformHelper.OpenUrl("https://github.com/petergeneric/unifi-protect-remux");
    }

    private void OnClose(object? sender, RoutedEventArgs e)
    {
        Close();
    }
}
