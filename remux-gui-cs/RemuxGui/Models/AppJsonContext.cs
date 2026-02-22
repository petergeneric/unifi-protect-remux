using System.Text.Json.Serialization;

namespace RemuxGui.Models;

[JsonSerializable(typeof(RemuxConfig))]
[JsonSerializable(typeof(ProgressEvent))]
[JsonSerializable(typeof(VersionInfo))]
internal partial class AppJsonContext : JsonSerializerContext
{
}
