using System;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using RemuxGui.ViewModels;
using RemuxGui.Views;

namespace RemuxGui;

public partial class App : Application
{
    public override void Initialize()
    {
        AvaloniaXamlLoader.Load(this);
    }

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            var viewModel = new MainViewModel();
            desktop.MainWindow = new MainWindow
            {
                DataContext = viewModel
            };

            if (desktop.Args is { Length: > 0 })
            {
                var ubvPaths = desktop.Args
                    .Where(a => a.EndsWith(".ubv", StringComparison.OrdinalIgnoreCase)
                             || a.EndsWith(".ubv.gz", StringComparison.OrdinalIgnoreCase));
                viewModel.AddFiles(ubvPaths);
            }
        }

        base.OnFrameworkInitializationCompleted();
    }

    private async void OnNativeAboutClick(object? sender, EventArgs e)
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop
            && desktop.MainWindow != null)
        {
            await AboutWindow.ShowAbout(desktop.MainWindow);
        }
    }
}
