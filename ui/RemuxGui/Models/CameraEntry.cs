using CommunityToolkit.Mvvm.ComponentModel;
using RemuxGui.Interop;

namespace RemuxGui.Models;

public partial class CameraEntry : ObservableObject
{
    [ObservableProperty]
    private string _macAddress = "";

    [ObservableProperty]
    private string _friendlyName = "";

    public string MacAddressFormatted => RemuxNative.FormatMac(MacAddress) ?? MacAddress;

    public string DisplayName => !string.IsNullOrWhiteSpace(FriendlyName) ? FriendlyName : MacAddressFormatted;

    public CameraEntry()
    {
    }

    public CameraEntry(string mac, string name)
    {
        _macAddress = NormalizeMac(mac);
        _friendlyName = name;
    }

    partial void OnMacAddressChanged(string value)
    {
        OnPropertyChanged(nameof(MacAddressFormatted));
        OnPropertyChanged(nameof(DisplayName));
    }

    partial void OnFriendlyNameChanged(string value)
    {
        OnPropertyChanged(nameof(DisplayName));
    }

    private static string NormalizeMac(string input)
    {
        return input.Replace(":", "").Replace("-", "").ToUpperInvariant();
    }
}
