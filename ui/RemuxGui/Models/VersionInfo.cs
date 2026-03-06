using System.Text.Json.Serialization;

namespace RemuxGui.Models;

public class VersionInfo
{
    [JsonPropertyName("version")]
    public string Version { get; set; } = "";

    [JsonPropertyName("git_commit")]
    public string GitCommit { get; set; } = "";

    [JsonIgnore]
    public string ShortCommit => GitCommit.Length > 10 ? GitCommit[..10] : GitCommit;
}
