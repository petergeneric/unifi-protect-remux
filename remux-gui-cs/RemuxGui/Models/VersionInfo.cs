using System.Text.Json.Serialization;

namespace RemuxGui.Models;

public class VersionInfo
{
    [JsonPropertyName("version")]
    public string Version { get; set; } = "";

    [JsonPropertyName("git_commit")]
    public string GitCommit { get; set; } = "";

    [JsonPropertyName("release_version")]
    public string ReleaseVersion { get; set; } = "";
}
