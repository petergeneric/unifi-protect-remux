using System.Diagnostics;
using System.Linq;
using System.Runtime.InteropServices;
using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Platform.Storage;
using RemuxGui.ViewModels;

namespace RemuxGui.Views;

public partial class FilesView : UserControl
{
    public FilesView()
    {
        InitializeComponent();
    }

    private MainViewModel? ViewModel => DataContext as MainViewModel;

    private async void OnBrowseFiles(object? sender, RoutedEventArgs e)
    {
        if (ViewModel == null) return;

        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var files = await topLevel.StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Select UBV files",
            AllowMultiple = true,
            FileTypeFilter = new[]
            {
                new FilePickerFileType("UBV files") { Patterns = new[] { "*.ubv", "*.ubv.gz" } },
                new FilePickerFileType("All files") { Patterns = new[] { "*" } }
            }
        });

        if (files.Count == 0) return;

        var paths = files
            .Select(f => f.TryGetLocalPath())
            .Where(p => p != null)
            .Cast<string>()
            .ToList();

        var warned = ViewModel.AddFiles(paths);
        if (warned.Count > 0)
        {
            var mainWindow = topLevel as MainWindow;
            if (mainWindow != null)
            {
                await mainWindow.ShowLowResWarning(warned);
            }
        }
    }

    private void OnOutputFileClick(object? sender, RoutedEventArgs e)
    {
        if (sender is not Button button || button.Tag is not string path)
            return;

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
