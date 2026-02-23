using Avalonia.Controls;
using Avalonia.Interactivity;

namespace RemuxGui.Views;

public partial class CamerasView : UserControl
{
    public CamerasView()
    {
        InitializeComponent();
    }

    private void OnSaveClick(object? sender, RoutedEventArgs e)
    {
        // De-focus the TextBox so the user sees the edit is "committed"
        TopLevel.GetTopLevel(this)?.FocusManager?.ClearFocus();
    }
}
