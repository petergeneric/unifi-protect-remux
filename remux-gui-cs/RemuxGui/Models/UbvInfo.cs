using System;
using System.Collections.Generic;
using System.Text.Json;

namespace RemuxGui.Models;

public record PartitionHeaderInfo
{
    public int Index { get; init; }
    public ulong? FileOffset { get; init; }
    public ulong? Dts { get; init; }
    public uint? ClockRate { get; init; }
    public ushort? FormatCode { get; init; }
    public int TotalEntries { get; init; }
    public List<(string Label, int Count)> EntryCounts { get; init; } = new();
}

public class UbvInfoTreeNode
{
    public string Label { get; }
    public List<UbvInfoTreeNode> Children { get; } = new();
    public List<UbvInfoEntry> Entries { get; } = new();
    public bool IsPartition { get; }
    public PartitionHeaderInfo? Header { get; set; }

    public UbvInfoTreeNode(string label, bool isPartition = false)
    {
        Label = label;
        IsPartition = isPartition;
    }
}

public class UbvInfoEntry
{
    public string Type { get; init; } = "";
    public ushort? TrackId { get; init; }
    public bool? Keyframe { get; init; }
    public string KeyframeLabel => Keyframe == true ? "\u2713" : "";
    public ulong? Offset { get; init; }
    public uint? Size { get; init; }
    public ulong? Dts { get; init; }
    public long? Cts { get; init; }
    public ulong? Wc { get; init; }
    public uint? ClockRate { get; init; }
    public ushort? Sequence { get; init; }
    public string? PacketPosition { get; init; }
}

public static class UbvInfoParser
{
    private static readonly Dictionary<ushort, string> TrackNames = new()
    {
        [7] = "Video (H.264)",
        [1003] = "Video (HEVC)",
        [1004] = "Video (AV1)",
        [1000] = "Audio (AAC)",
        [1001] = "Audio (Raw)",
        [1002] = "Audio (Opus)",
    };

    private static string GetTrackName(ushort trackId)
    {
        return TrackNames.TryGetValue(trackId, out var name)
            ? name
            : $"Track {trackId}";
    }

    public static List<UbvInfoTreeNode> Parse(string json)
    {
        var roots = new List<UbvInfoTreeNode>();
        using var doc = JsonDocument.Parse(json);
        var root = doc.RootElement;

        if (!root.TryGetProperty("partitions", out var partitions))
            return roots;

        foreach (var partition in partitions.EnumerateArray())
        {
            var index = partition.GetProperty("index").GetInt32();
            var partNode = new UbvInfoTreeNode($"Partition {index}", isPartition: true);

            // Group entries by type
            var frameGroups = new Dictionary<ushort, List<UbvInfoEntry>>();
            var clockSyncs = new List<UbvInfoEntry>();
            var motionEntries = new List<UbvInfoEntry>();
            var smartEventEntries = new List<UbvInfoEntry>();
            var jpegEntries = new List<UbvInfoEntry>();
            var skipEntries = new List<UbvInfoEntry>();
            var talkbackEntries = new List<UbvInfoEntry>();

            if (partition.TryGetProperty("entries", out var entries))
            {
                foreach (var entry in entries.EnumerateArray())
                {
                    if (entry.TryGetProperty("Frame", out var frame))
                    {
                        var trackId = frame.GetProperty("track_id").GetUInt16();
                        var row = new UbvInfoEntry
                        {
                            Type = frame.TryGetProperty("type_char", out var tc) ? tc.GetString() ?? "?" : "?",
                            TrackId = trackId,
                            Keyframe = frame.TryGetProperty("keyframe", out var kf) ? kf.GetBoolean() : null,
                            Offset = frame.TryGetProperty("data_offset", out var off) ? off.GetUInt64() : null,
                            Size = frame.TryGetProperty("data_size", out var sz) ? sz.GetUInt32() : null,
                            Dts = frame.TryGetProperty("dts", out var dts) ? dts.GetUInt64() : null,
                            Cts = frame.TryGetProperty("cts", out var cts) ? cts.GetInt64() : null,
                            Wc = frame.TryGetProperty("wc", out var wc) ? wc.GetUInt64() : null,
                            ClockRate = frame.TryGetProperty("clock_rate", out var cr) ? cr.GetUInt32() : null,
                            Sequence = frame.TryGetProperty("sequence", out var seq) ? seq.GetUInt16() : null,
                            PacketPosition = frame.TryGetProperty("packet_position", out var pp) ? pp.GetString() : null,
                        };

                        if (!frameGroups.TryGetValue(trackId, out var list))
                        {
                            list = new List<UbvInfoEntry>();
                            frameGroups[trackId] = list;
                        }
                        list.Add(row);
                    }
                    else if (entry.TryGetProperty("ClockSync", out var cs))
                    {
                        clockSyncs.Add(new UbvInfoEntry
                        {
                            Type = "CS",
                            Dts = cs.TryGetProperty("sc_dts", out var dts) ? dts.GetUInt64() : null,
                            ClockRate = cs.TryGetProperty("sc_rate", out var cr) ? cr.GetUInt32() : null,
                            Wc = cs.TryGetProperty("wc_ms", out var wc) ? wc.GetUInt64() : null,
                        });
                    }
                    else
                    {
                        // Motion, SmartEvent, Jpeg, Skip, Talkback — all MetadataRecord
                        ParseMetadataEntry(entry, "Motion", "M", motionEntries);
                        ParseMetadataEntry(entry, "SmartEvent", "SE", smartEventEntries);
                        ParseMetadataEntry(entry, "Jpeg", "J", jpegEntries);
                        ParseMetadataEntry(entry, "Skip", "Skip", skipEntries);
                        ParseMetadataEntry(entry, "Talkback", "TB", talkbackEntries);
                    }
                }
            }

            // Build child nodes — frames grouped by track
            foreach (var (trackId, frames) in frameGroups)
            {
                var trackName = GetTrackName(trackId);
                var child = new UbvInfoTreeNode($"{trackName} ({frames.Count})");
                child.Entries.AddRange(frames);
                partNode.Children.Add(child);
            }

            AddGroupNode(partNode, "Clock Syncs", clockSyncs);
            AddGroupNode(partNode, "Motion", motionEntries);
            AddGroupNode(partNode, "Smart Events", smartEventEntries);
            AddGroupNode(partNode, "JPEG", jpegEntries);
            AddGroupNode(partNode, "Skip", skipEntries);
            AddGroupNode(partNode, "Talkback", talkbackEntries);

            // Partition-level entries: flattened list of all child entries
            int totalEntries = 0;
            var entryCounts = new List<(string, int)>();
            foreach (var child in partNode.Children)
            {
                partNode.Entries.AddRange(child.Entries);
                entryCounts.Add((child.Label, child.Entries.Count));
                totalEntries += child.Entries.Count;
            }

            // Parse partition header
            var headerInfo = new PartitionHeaderInfo
            {
                Index = index,
                TotalEntries = totalEntries,
                EntryCounts = entryCounts,
            };
            if (partition.TryGetProperty("header", out var header))
            {
                headerInfo = headerInfo with
                {
                    FileOffset = header.TryGetProperty("file_offset", out var fo) ? fo.GetUInt64() : null,
                    Dts = header.TryGetProperty("dts", out var hdts) ? hdts.GetUInt64() : null,
                    ClockRate = header.TryGetProperty("clock_rate", out var hcr) ? hcr.GetUInt32() : null,
                    FormatCode = header.TryGetProperty("format_code", out var fc) ? fc.GetUInt16() : null,
                };
            }
            partNode.Header = headerInfo;

            roots.Add(partNode);
        }

        return roots;
    }

    private static void ParseMetadataEntry(JsonElement entry, string key, string displayType, List<UbvInfoEntry> target)
    {
        if (!entry.TryGetProperty(key, out var meta))
            return;

        target.Add(new UbvInfoEntry
        {
            Type = displayType,
            TrackId = meta.TryGetProperty("track_id", out var tid) ? tid.GetUInt16() : null,
            Keyframe = meta.TryGetProperty("keyframe", out var kf) ? kf.GetBoolean() : null,
            Offset = meta.TryGetProperty("file_offset", out var fo)
                ? fo.GetUInt64()
                : (meta.TryGetProperty("data_offset", out var doff) ? doff.GetUInt64() : null),
            Size = meta.TryGetProperty("data_size", out var sz) ? sz.GetUInt32() : null,
            Dts = meta.TryGetProperty("dts", out var dts) ? dts.GetUInt64() : null,
            ClockRate = meta.TryGetProperty("clock_rate", out var cr) ? cr.GetUInt32() : null,
            Sequence = meta.TryGetProperty("sequence", out var seq) ? seq.GetUInt16() : null,
        });
    }

    private static void AddGroupNode(UbvInfoTreeNode parent, string label, List<UbvInfoEntry> entries)
    {
        if (entries.Count == 0) return;
        var node = new UbvInfoTreeNode($"{label} ({entries.Count})");
        node.Entries.AddRange(entries);
        parent.Children.Add(node);
    }
}
