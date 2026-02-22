using System.Text.Json.Serialization;

namespace RemuxGui.Models;

public class LicenseEntry
{
    [JsonPropertyName("name")]
    public string Name { get; set; } = "";

    [JsonPropertyName("version")]
    public string Version { get; set; } = "";

    [JsonPropertyName("license")]
    public string License { get; set; } = "";

    [JsonPropertyName("authors")]
    public string Authors { get; set; } = "";

    [JsonPropertyName("repository")]
    public string Repository { get; set; } = "";
}
