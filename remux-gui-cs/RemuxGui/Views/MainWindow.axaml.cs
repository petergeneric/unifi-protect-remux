using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Media;
using Avalonia.VisualTree;
using Avalonia.Platform.Storage;
using MsBox.Avalonia;
using MsBox.Avalonia.Enums;
using RemuxGui.ViewModels;

namespace RemuxGui.Views;

public partial class MainWindow : Window
{
    private static readonly IBrush AccentBorderBrush = new SolidColorBrush(Color.Parse("#6C8CFF"));

    public MainWindow()
    {
        InitializeComponent();

        AddHandler(DragDrop.DropEvent, OnDrop);
        AddHandler(DragDrop.DragOverEvent, OnDragOver);
        AddHandler(DragDrop.DragEnterEvent, OnDragEnter);
        AddHandler(DragDrop.DragLeaveEvent, OnDragLeave);

        DataContextChanged += (_, _) =>
        {
            if (ViewModel != null)
            {
                ViewModel.OpenUbvInfoRequested = (ubvPath, filename, json) =>
                    UbvInfoWindow.ShowUbvInfo(this, ubvPath, filename, json);
            }
        };
    }

    private MainViewModel? ViewModel => DataContext as MainViewModel;

    private Border? FindFileListBorder()
    {
        return this.FindDescendantOfType<FilesView>()
            ?.FindControl<Border>("FileListBorder");
    }

    private void SetDropHighlight(bool active)
    {
        var border = FindFileListBorder();
        if (border == null) return;

        if (active)
        {
            border.BorderBrush = AccentBorderBrush;
            border.BorderThickness = new Thickness(2);
        }
        else
        {
            border.BorderBrush = (IBrush)this.FindResource("SystemControlForegroundBaseMediumLowBrush")!;
            border.BorderThickness = new Thickness(1);
        }
    }

    private void OnDragOver(object? sender, DragEventArgs e)
    {
        e.DragEffects = DragDropEffects.Copy;
    }

    private void OnDragEnter(object? sender, DragEventArgs e)
    {
        SetDropHighlight(true);
    }

    private void OnDragLeave(object? sender, DragEventArgs e)
    {
        SetDropHighlight(false);
    }

    private async void OnDrop(object? sender, DragEventArgs e)
    {
        SetDropHighlight(false);

        if (ViewModel == null) return;

        var files = e.DataTransfer.TryGetFiles();
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
