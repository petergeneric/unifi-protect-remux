using System.Linq;
using CommunityToolkit.Mvvm.ComponentModel;

namespace RemuxGui.Models;

public partial class CameraEntry : ObservableObject
{
    [ObservableProperty]
    private string _macAddress = "";

    [ObservableProperty]
    private string _friendlyName = "";

    public string MacAddressFormatted
    {
        get
        {
            if (MacAddress.Length != 12)
                return MacAddress;
            return string.Join(":", Enumerable.Range(0, 6).Select(i => MacAddress.Substring(i * 2, 2)));
        }
    }

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
