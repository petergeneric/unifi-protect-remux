using System.Text.Json.Serialization;

namespace RemuxGui.Models;

public class RemuxConfig
{
    [JsonPropertyName("with_audio")]
    public bool WithAudio { get; set; } = true;

    [JsonPropertyName("with_video")]
    public bool WithVideo { get; set; } = true;

    [JsonPropertyName("force_rate")]
    public uint ForceRate { get; set; } = 0;

    [JsonPropertyName("fast_start")]
    public bool FastStart { get; set; } = false;

    [JsonPropertyName("output_folder")]
    public string OutputFolder { get; set; } = "SRC-FOLDER";

    [JsonPropertyName("mp4")]
    public bool Mp4 { get; set; } = true;

    [JsonPropertyName("video_track")]
    public ushort VideoTrack { get; set; } = 0;
}
