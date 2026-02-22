using System.Collections.Generic;
using System.Linq;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Platform.Storage;
using MsBox.Avalonia;
using MsBox.Avalonia.Enums;
using RemuxGui.ViewModels;

namespace RemuxGui.Views;

public partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();

        AddHandler(DragDrop.DropEvent, OnDrop);
        AddHandler(DragDrop.DragOverEvent, OnDragOver);

        VersionFooter.PointerPressed += OnVersionClick;
    }

    private MainViewModel? ViewModel => DataContext as MainViewModel;

    private void OnDragOver(object? sender, DragEventArgs e)
    {
        e.DragEffects = DragDropEffects.Copy;
    }

    private async void OnDrop(object? sender, DragEventArgs e)
    {
        if (ViewModel == null) return;

        var files = e.Data.GetFiles();
        if (files == null) return;

        var paths = files
            .Select(f => f.TryGetLocalPath())
            .Where(p => p != null)
            .Cast<string>()
            .ToList();

        if (paths.Count == 0) return;

        var warned = ViewModel.AddFiles(paths);
        if (warned.Count > 0)
        {
            await ShowLowResWarning(warned);
        }
    }

    private async void OnVersionClick(object? sender, PointerPressedEventArgs e)
    {
        await AboutWindow.ShowAbout(this);
    }

    internal async System.Threading.Tasks.Task ShowLowResWarning(List<string> warnedPaths)
    {
        var fileNames = warnedPaths
            .Select(p => System.IO.Path.GetFileName(p))
            .ToList();

        var message = $"The following file(s) appear to be low-resolution recordings " +
                      $"that do not contain the raw camera data:\n\n" +
                      $"{string.Join("\n", fileNames)}\n\n" +
                      $"These files are unlikely to produce useful results, and the " +
                      $"remux tool does not fully support them.\n\n" +
                      $"Add them anyway?";

        var box = MessageBoxManager.GetMessageBoxStandard(
            "Low-Resolution File Warning",
            message,
            ButtonEnum.YesNo,
            MsBox.Avalonia.Enums.Icon.Warning);

        var result = await box.ShowWindowDialogAsync(this);
        if (result == ButtonResult.Yes)
        {
            ViewModel?.AddWarnedFiles(warnedPaths);
        }
    }
}
