using System.Text.Json.Serialization;

namespace RemuxGui.Models;

[JsonSerializable(typeof(RemuxConfig))]
[JsonSerializable(typeof(ProgressEvent))]
[JsonSerializable(typeof(VersionInfo))]
[JsonSerializable(typeof(CameraData))]
[JsonSerializable(typeof(CameraDataEntry))]
[JsonSerializable(typeof(LicenseEntry[]))]
internal partial class AppJsonContext : JsonSerializerContext
{
}
