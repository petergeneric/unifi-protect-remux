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

/// <summary>
/// Parses the structured UBV info JSON returned by the FFI crate.
/// </summary>
public static class UbvInfoParser
{
    public static List<UbvInfoTreeNode> Parse(string json)
    {
        var roots = new List<UbvInfoTreeNode>();
        using var doc = JsonDocument.Parse(json);
        var root = doc.RootElement;

        if (!root.TryGetProperty("partitions", out var partitions))
            return roots;

        foreach (var partition in partitions.EnumerateArray())
        {
            var label = partition.TryGetProperty("label", out var labelEl)
                ? labelEl.GetString() ?? "Partition"
                : "Partition";
            var partNode = new UbvInfoTreeNode(label, isPartition: true);

            // Parse pre-built groups from FFI
            if (partition.TryGetProperty("groups", out var groups))
            {
                foreach (var group in groups.EnumerateArray())
                {
                    var groupLabel = group.TryGetProperty("label", out var gl)
                        ? gl.GetString() ?? "Unknown"
                        : "Unknown";
                    var child = new UbvInfoTreeNode(groupLabel);

                    if (group.TryGetProperty("entries", out var entries))
                    {
                        foreach (var entry in entries.EnumerateArray())
                        {
                            child.Entries.Add(ParseEntry(entry));
                        }
                    }

                    partNode.Children.Add(child);
                }
            }

            // Parse header info
            if (partition.TryGetProperty("header", out var header))
            {
                var entryCounts = new List<(string, int)>();
                if (header.TryGetProperty("entry_counts", out var counts))
                {
                    foreach (var count in counts.EnumerateArray())
                    {
                        var countLabel = count.TryGetProperty("label", out var cl)
                            ? cl.GetString() ?? ""
                            : "";
                        var countValue = count.TryGetProperty("count", out var cv)
                            ? cv.GetInt32()
                            : 0;
                        entryCounts.Add((countLabel, countValue));
                    }
                }

                partNode.Header = new PartitionHeaderInfo
                {
                    Index = header.TryGetProperty("index", out var idx) ? idx.GetInt32() : 0,
                    FileOffset = header.TryGetProperty("file_offset", out var fo) ? fo.GetUInt64() : null,
                    Dts = header.TryGetProperty("dts", out var dts) ? dts.GetUInt64() : null,
                    ClockRate = header.TryGetProperty("clock_rate", out var cr) ? cr.GetUInt32() : null,
                    FormatCode = header.TryGetProperty("format_code", out var fc) ? fc.GetUInt16() : null,
                    TotalEntries = header.TryGetProperty("total_entries", out var te) ? te.GetInt32() : 0,
                    EntryCounts = entryCounts,
                };
            }

            roots.Add(partNode);
        }

        return roots;
    }

    private static UbvInfoEntry ParseEntry(JsonElement entry)
    {
        return new UbvInfoEntry
        {
            Type = entry.TryGetProperty("type", out var t) ? t.GetString() ?? "?" : "?",
            TrackId = entry.TryGetProperty("track_id", out var tid) ? tid.GetUInt16() : null,
            Keyframe = entry.TryGetProperty("keyframe", out var kf) ? kf.GetBoolean() : null,
            Offset = entry.TryGetProperty("offset", out var off) ? off.GetUInt64() : null,
            Size = entry.TryGetProperty("size", out var sz) ? sz.GetUInt32() : null,
            Dts = entry.TryGetProperty("dts", out var dts) ? dts.GetUInt64() : null,
            Cts = entry.TryGetProperty("cts", out var cts) ? cts.GetInt64() : null,
            Wc = entry.TryGetProperty("wc", out var wc) ? wc.GetUInt64() : null,
            ClockRate = entry.TryGetProperty("clock_rate", out var cr) ? cr.GetUInt32() : null,
            Sequence = entry.TryGetProperty("sequence", out var seq) ? seq.GetUInt16() : null,
            PacketPosition = entry.TryGetProperty("packet_position", out var pp) ? pp.GetString() : null,
        };
    }
}
