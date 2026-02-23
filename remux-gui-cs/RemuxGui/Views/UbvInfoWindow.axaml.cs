using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Templates;
using Avalonia.Interactivity;
using Avalonia.Media;
using flate2 = System.IO.Compression;
using RemuxGui.Models;

namespace RemuxGui.Views;

public partial class UbvInfoWindow : Window
{
    private static readonly GridLength KfWidthVisible = new(28);
    private static readonly GridLength KfWidthHidden = new(0);

    private static readonly FontFamily MonoFont =
        new("Cascadia Mono, Consolas, Menlo, monospace");

    private string _ubvPath = "";
    private string _json = "";
    private bool _showKf;
    private bool _templateInitialized;

    public UbvInfoWindow()
    {
        InitializeComponent();
    }

    private void OnCloseWindow(object? sender, EventArgs e)
    {
        Close();
    }

    public static void ShowUbvInfo(Window owner, string ubvPath, string filename, string json)
    {
        var window = new UbvInfoWindow();
        window.Title = $"UBV Info \u2014 {filename}";
        window._ubvPath = ubvPath;
        window._json = json;

        var roots = UbvInfoParser.Parse(json);
        window.InfoTree.ItemsSource = roots;

        window.Show();
    }

    private void OnTreeSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (InfoTree.SelectedItem is not UbvInfoTreeNode node)
            return;

        if (node.IsPartition && node.Header != null)
        {
            ShowPartitionView(node.Header);
        }
        else
        {
            ShowTableView(node);
        }
    }

    private void ShowPartitionView(PartitionHeaderInfo header)
    {
        TablePanel.IsVisible = false;
        PartitionPanel.IsVisible = true;

        PartitionTitle.Text = $"Partition {header.Index}";
        HdrFileOffset.Text = header.FileOffset?.ToString() ?? "";
        HdrDts.Text = header.Dts?.ToString() ?? "";
        HdrClockRate.Text = header.ClockRate?.ToString() ?? "";
        HdrFormatCode.Text = header.FormatCode != null ? $"0x{header.FormatCode:X4}" : "";
        HdrEntriesLabel.Text = $"ENTRIES ({header.TotalEntries})";

        EntryCountsPanel.Children.Clear();
        foreach (var (label, count) in header.EntryCounts)
        {
            var row = new Grid();
            row.ColumnDefinitions.Add(new ColumnDefinition(1, GridUnitType.Star));
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            var labelTb = new TextBlock
            {
                Text = label,
                FontSize = 12,
            };
            Grid.SetColumn(labelTb, 0);
            row.Children.Add(labelTb);

            var countTb = new TextBlock
            {
                Text = count.ToString(),
                FontSize = 12,
                FontFamily = MonoFont,
                Foreground = (Avalonia.Media.IBrush?)this.FindResource("SystemControlForegroundBaseMediumBrush"),
            };
            Grid.SetColumn(countTb, 1);
            row.Children.Add(countTb);

            EntryCountsPanel.Children.Add(row);
        }
    }

    private void ShowTableView(UbvInfoTreeNode node)
    {
        PartitionPanel.IsVisible = false;
        TablePanel.IsVisible = true;

        bool hasTrue = false, hasFalse = false;
        foreach (var entry in node.Entries)
        {
            if (entry.Keyframe == true) hasTrue = true;
            else if (entry.Keyframe == false) hasFalse = true;
            if (hasTrue && hasFalse) break;
        }

        var showKf = hasTrue && hasFalse;
        if (!_templateInitialized || showKf != _showKf)
        {
            _showKf = showKf;
            _templateInitialized = true;
            HeaderGrid.ColumnDefinitions[2].Width = showKf ? KfWidthVisible : KfWidthHidden;
            EntryList.ItemTemplate = BuildRowTemplate(showKf);
        }

        EntryList.ItemsSource = node.Entries;
    }

    private static FuncDataTemplate<UbvInfoEntry> BuildRowTemplate(bool showKf)
    {
        var kfWidth = showKf ? KfWidthVisible : KfWidthHidden;

        return new FuncDataTemplate<UbvInfoEntry>((entry, _) =>
        {
            var grid = new Grid { Margin = new Thickness(8, 0, 0, 0) };
            grid.ColumnDefinitions.Add(new ColumnDefinition(40, GridUnitType.Pixel));
            grid.ColumnDefinitions.Add(new ColumnDefinition(40, GridUnitType.Pixel));
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = kfWidth });
            grid.ColumnDefinitions.Add(new ColumnDefinition(1, GridUnitType.Star));
            grid.ColumnDefinitions.Add(new ColumnDefinition(1, GridUnitType.Star));
            grid.ColumnDefinitions.Add(new ColumnDefinition(1, GridUnitType.Star));
            grid.ColumnDefinitions.Add(new ColumnDefinition(1, GridUnitType.Star));
            grid.ColumnDefinitions.Add(new ColumnDefinition(1, GridUnitType.Star));
            grid.ColumnDefinitions.Add(new ColumnDefinition(1, GridUnitType.Star));
            grid.ColumnDefinitions.Add(new ColumnDefinition(40, GridUnitType.Pixel));
            grid.ColumnDefinitions.Add(new ColumnDefinition(60, GridUnitType.Pixel));

            void AddCell(int col, string? text)
            {
                var tb = new TextBlock
                {
                    Text = text ?? "",
                    FontFamily = MonoFont,
                    FontSize = 12,
                    VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center,
                    TextTrimming = TextTrimming.CharacterEllipsis,
                };
                Grid.SetColumn(tb, col);
                grid.Children.Add(tb);
            }

            if (entry is null)
                return grid;

            AddCell(0, entry.Type);
            AddCell(1, entry.TrackId?.ToString());
            if (showKf)
                AddCell(2, entry.KeyframeLabel);
            AddCell(3, entry.Offset?.ToString());
            AddCell(4, entry.Size?.ToString());
            AddCell(5, entry.Dts?.ToString());
            AddCell(6, entry.Cts?.ToString());
            AddCell(7, entry.Wc?.ToString());
            AddCell(8, entry.ClockRate?.ToString());
            AddCell(9, entry.Sequence?.ToString());
            AddCell(10, entry.PacketPosition);

            return grid;
        });
    }

    private void OnSaveJson(object? sender, RoutedEventArgs e)
    {
        if (string.IsNullOrEmpty(_json) || string.IsNullOrEmpty(_ubvPath))
            return;

        try
        {
            var outputPath = _ubvPath + ".json.gz";
            using (var file = File.Create(outputPath))
            using (var gz = new flate2.GZipStream(file, flate2.CompressionLevel.Optimal))
            using (var writer = new StreamWriter(gz))
            {
                writer.Write(_json);
            }

            RevealInFileBrowser(outputPath);
        }
        catch
        {
            // Best-effort
        }
    }

    private static void RevealInFileBrowser(string path)
    {
        try
        {
            if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
                Process.Start("explorer.exe", $"/select,\"{path}\"");
            else if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX))
                Process.Start("open", $"-R \"{path}\"");
            else
                Process.Start("xdg-open", Path.GetDirectoryName(path) ?? path);
        }
        catch
        {
            // Silently ignore
        }
    }
}
