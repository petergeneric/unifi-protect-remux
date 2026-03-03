import Foundation

// MARK: - Partition header metadata

struct PartitionHeaderInfo {
    let index: Int
    var fileOffset: UInt64?
    var dts: UInt64?
    var clockRate: UInt32?
    var formatCode: UInt16?
    let totalEntries: Int
    let entryCounts: [(label: String, count: Int)]
}

// MARK: - Entry row (displayed in table)

struct UbvInfoEntry: Identifiable {
    let id = UUID()
    let type: String
    let trackId: UInt16?
    let keyframe: Bool?
    let offset: UInt64?
    let size: UInt32?
    let dts: UInt64?
    let cts: Int64?
    let wc: UInt64?
    let clockRate: UInt32?
    let sequence: UInt16?
    let packetPosition: String?

    var keyframeLabel: String { keyframe == true ? "\u{2713}" : "" }
}

// MARK: - Tree node

final class UbvInfoTreeNode: Identifiable {
    let id = UUID()
    let label: String
    var children: [UbvInfoTreeNode] = []
    var entries: [UbvInfoEntry] = []
    let isPartition: Bool
    var header: PartitionHeaderInfo?

    init(label: String, isPartition: Bool = false) {
        self.label = label
        self.isPartition = isPartition
    }

    var optionalChildren: [UbvInfoTreeNode]? {
        children.isEmpty ? nil : children
    }
}

// MARK: - Parser

enum UbvInfoParser {

    private static let trackNames: [UInt16: String] = [
        7: "Video (H.264)",
        1003: "Video (HEVC)",
        1004: "Video (AV1)",
        1000: "Audio (AAC)",
        1001: "Audio (Raw)",
        1002: "Audio (Opus)",
    ]

    private static func trackName(for id: UInt16) -> String {
        trackNames[id] ?? "Track \(id)"
    }

    static func parse(json: String) -> [UbvInfoTreeNode] {
        guard let data = json.data(using: .utf8),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let partitions = root["partitions"] as? [[String: Any]] else {
            return []
        }

        var nodes: [UbvInfoTreeNode] = []

        for partition in partitions {
            guard let index = partition["index"] as? Int else { continue }
            let partNode = UbvInfoTreeNode(label: "Partition \(index)", isPartition: true)

            // Group entries by type
            var frameGroups: [UInt16: [UbvInfoEntry]] = [:]
            var clockSyncs: [UbvInfoEntry] = []
            var motionEntries: [UbvInfoEntry] = []
            var smartEventEntries: [UbvInfoEntry] = []
            var jpegEntries: [UbvInfoEntry] = []
            var skipEntries: [UbvInfoEntry] = []
            var talkbackEntries: [UbvInfoEntry] = []

            if let entries = partition["entries"] as? [[String: Any]] {
                for entry in entries {
                    if let frame = entry["Frame"] as? [String: Any] {
                        let trackId = UInt16(truncatingIfNeeded: (frame["track_id"] as? Int) ?? 0)
                        let row = UbvInfoEntry(
                            type: (frame["type_char"] as? String) ?? "?",
                            trackId: trackId,
                            keyframe: frame["keyframe"] as? Bool,
                            offset: (frame["data_offset"] as? UInt64) ?? (frame["data_offset"] as? Int).map(UInt64.init),
                            size: (frame["data_size"] as? UInt32) ?? (frame["data_size"] as? Int).map(UInt32.init),
                            dts: (frame["dts"] as? UInt64) ?? (frame["dts"] as? Int).map(UInt64.init),
                            cts: (frame["cts"] as? Int64) ?? (frame["cts"] as? Int).map(Int64.init),
                            wc: (frame["wc"] as? UInt64) ?? (frame["wc"] as? Int).map(UInt64.init),
                            clockRate: (frame["clock_rate"] as? UInt32) ?? (frame["clock_rate"] as? Int).map(UInt32.init),
                            sequence: (frame["sequence"] as? UInt16) ?? (frame["sequence"] as? Int).map(UInt16.init),
                            packetPosition: frame["packet_position"] as? String
                        )
                        frameGroups[trackId, default: []].append(row)
                    } else if let cs = entry["ClockSync"] as? [String: Any] {
                        clockSyncs.append(UbvInfoEntry(
                            type: "CS",
                            trackId: nil,
                            keyframe: nil,
                            offset: nil,
                            size: nil,
                            dts: (cs["sc_dts"] as? UInt64) ?? (cs["sc_dts"] as? Int).map(UInt64.init),
                            cts: nil,
                            wc: (cs["wc_ms"] as? UInt64) ?? (cs["wc_ms"] as? Int).map(UInt64.init),
                            clockRate: (cs["sc_rate"] as? UInt32) ?? (cs["sc_rate"] as? Int).map(UInt32.init),
                            sequence: nil,
                            packetPosition: nil
                        ))
                    } else {
                        parseMetadataEntry(entry, key: "Motion", displayType: "M", into: &motionEntries)
                        parseMetadataEntry(entry, key: "SmartEvent", displayType: "SE", into: &smartEventEntries)
                        parseMetadataEntry(entry, key: "Jpeg", displayType: "J", into: &jpegEntries)
                        parseMetadataEntry(entry, key: "Skip", displayType: "Skip", into: &skipEntries)
                        parseMetadataEntry(entry, key: "Talkback", displayType: "TB", into: &talkbackEntries)
                    }
                }
            }

            // Build child nodes — frames grouped by track
            for trackId in frameGroups.keys.sorted() {
                let frames = frameGroups[trackId]!
                let name = trackName(for: trackId)
                let child = UbvInfoTreeNode(label: "\(name) (\(frames.count))")
                child.entries = frames
                partNode.children.append(child)
            }

            addGroupNode(to: partNode, label: "Clock Syncs", entries: clockSyncs)
            addGroupNode(to: partNode, label: "Motion", entries: motionEntries)
            addGroupNode(to: partNode, label: "Smart Events", entries: smartEventEntries)
            addGroupNode(to: partNode, label: "JPEG", entries: jpegEntries)
            addGroupNode(to: partNode, label: "Skip", entries: skipEntries)
            addGroupNode(to: partNode, label: "Talkback", entries: talkbackEntries)

            // Build partition header info
            var totalEntries = 0
            var entryCounts: [(String, Int)] = []
            for child in partNode.children {
                partNode.entries.append(contentsOf: child.entries)
                entryCounts.append((child.label, child.entries.count))
                totalEntries += child.entries.count
            }

            var headerInfo = PartitionHeaderInfo(
                index: index,
                totalEntries: totalEntries,
                entryCounts: entryCounts
            )

            if let header = partition["header"] as? [String: Any] {
                headerInfo.fileOffset = (header["file_offset"] as? UInt64) ?? (header["file_offset"] as? Int).map(UInt64.init)
                headerInfo.dts = (header["dts"] as? UInt64) ?? (header["dts"] as? Int).map(UInt64.init)
                headerInfo.clockRate = (header["clock_rate"] as? UInt32) ?? (header["clock_rate"] as? Int).map(UInt32.init)
                headerInfo.formatCode = (header["format_code"] as? UInt16) ?? (header["format_code"] as? Int).map(UInt16.init)
            }
            partNode.header = headerInfo

            nodes.append(partNode)
        }

        return nodes
    }

    private static func parseMetadataEntry(
        _ entry: [String: Any],
        key: String,
        displayType: String,
        into target: inout [UbvInfoEntry]
    ) {
        guard let meta = entry[key] as? [String: Any] else { return }

        let offset: UInt64? = {
            if let fo = (meta["file_offset"] as? UInt64) ?? (meta["file_offset"] as? Int).map(UInt64.init) {
                return fo
            }
            return (meta["data_offset"] as? UInt64) ?? (meta["data_offset"] as? Int).map(UInt64.init)
        }()

        target.append(UbvInfoEntry(
            type: displayType,
            trackId: (meta["track_id"] as? UInt16) ?? (meta["track_id"] as? Int).map(UInt16.init),
            keyframe: meta["keyframe"] as? Bool,
            offset: offset,
            size: (meta["data_size"] as? UInt32) ?? (meta["data_size"] as? Int).map(UInt32.init),
            dts: (meta["dts"] as? UInt64) ?? (meta["dts"] as? Int).map(UInt64.init),
            cts: nil,
            wc: nil,
            clockRate: (meta["clock_rate"] as? UInt32) ?? (meta["clock_rate"] as? Int).map(UInt32.init),
            sequence: (meta["sequence"] as? UInt16) ?? (meta["sequence"] as? Int).map(UInt16.init),
            packetPosition: nil
        ))
    }

    private static func addGroupNode(to parent: UbvInfoTreeNode, label: String, entries: [UbvInfoEntry]) {
        guard !entries.isEmpty else { return }
        let node = UbvInfoTreeNode(label: "\(label) (\(entries.count))")
        node.entries = entries
        parent.children.append(node)
    }
}
