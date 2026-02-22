using System.Text.Json.Serialization;

namespace RemuxGui.Models;

[JsonSerializable(typeof(RemuxConfig))]
[JsonSerializable(typeof(ProgressEvent))]
[JsonSerializable(typeof(VersionInfo))]
[JsonSerializable(typeof(CameraData))]
[JsonSerializable(typeof(CameraDataEntry))]
internal partial class AppJsonContext : JsonSerializerContext
{
}
