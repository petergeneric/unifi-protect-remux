using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace RemuxGui.Models;

public class CameraData
{
    [JsonPropertyName("cameras")]
    public List<CameraDataEntry> Cameras { get; set; } = new();
}

public class CameraDataEntry
{
    [JsonPropertyName("mac")]
    public string Mac { get; set; } = "";

    [JsonPropertyName("name")]
    public string Name { get; set; } = "";
}
