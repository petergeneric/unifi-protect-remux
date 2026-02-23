using System.Text.Json.Serialization;

namespace RemuxGui.Models;

public class ProgressEvent
{
    [JsonPropertyName("type")]
    public string Type { get; set; } = "";

    // Log fields
    [JsonPropertyName("level")]
    public string? Level { get; set; }

    [JsonPropertyName("message")]
    public string? Message { get; set; }

    // FileStarted / FileCompleted fields
    [JsonPropertyName("path")]
    public string? Path { get; set; }

    // PartitionsFound fields
    [JsonPropertyName("count")]
    public int? Count { get; set; }

    // PartitionStarted fields
    [JsonPropertyName("index")]
    public int? Index { get; set; }

    [JsonPropertyName("total")]
    public int? Total { get; set; }

    // PartitionError fields
    [JsonPropertyName("error")]
    public string? Error { get; set; }

    // FileCompleted fields
    [JsonPropertyName("outputs")]
    public string[]? Outputs { get; set; }

    [JsonPropertyName("errors")]
    public string[]? Errors { get; set; }
}
