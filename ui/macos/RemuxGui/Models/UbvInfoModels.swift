import Foundation

// MARK: - Partition header metadata

struct PartitionHeaderInfo {
    let index: Int
    let fileOffset: UInt64?
    let dts: UInt64?
    let clockRate: UInt32?
    let formatCode: UInt16?
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

// MARK: - Parser (consumes structured JSON from FFI)

enum UbvInfoParser {

    static func parse(json: String) -> [UbvInfoTreeNode] {
        guard let data = json.data(using: .utf8),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let partitions = root["partitions"] as? [[String: Any]] else {
            return []
        }

        var nodes: [UbvInfoTreeNode] = []

        for partition in partitions {
            let label = (partition["label"] as? String) ?? "Partition"
            let partNode = UbvInfoTreeNode(label: label, isPartition: true)

            // Parse groups (pre-built by FFI)
            if let groups = partition["groups"] as? [[String: Any]] {
                for group in groups {
                    let groupLabel = (group["label"] as? String) ?? "Unknown"
                    let child = UbvInfoTreeNode(label: groupLabel)

                    if let entries = group["entries"] as? [[String: Any]] {
                        child.entries = entries.map(parseEntry)
                    }

                    partNode.children.append(child)
                }
            }

            // Parse header info
            if let header = partition["header"] as? [String: Any] {
                let index = (header["index"] as? Int) ?? 0
                let totalEntries = (header["total_entries"] as? Int) ?? 0

                var entryCounts: [(String, Int)] = []
                if let counts = header["entry_counts"] as? [[String: Any]] {
                    for count in counts {
                        let countLabel = (count["label"] as? String) ?? ""
                        let countValue = (count["count"] as? Int) ?? 0
                        entryCounts.append((countLabel, countValue))
                    }
                }

                partNode.header = PartitionHeaderInfo(
                    index: index,
                    fileOffset: uint64(from: header, key: "file_offset"),
                    dts: uint64(from: header, key: "dts"),
                    clockRate: uint32(from: header, key: "clock_rate"),
                    formatCode: uint16(from: header, key: "format_code"),
                    totalEntries: totalEntries,
                    entryCounts: entryCounts
                )
            }

            nodes.append(partNode)
        }

        return nodes
    }

    private static func parseEntry(_ dict: [String: Any]) -> UbvInfoEntry {
        UbvInfoEntry(
            type: (dict["type"] as? String) ?? "?",
            trackId: uint16(from: dict, key: "track_id"),
            keyframe: dict["keyframe"] as? Bool,
            offset: uint64(from: dict, key: "offset"),
            size: uint32(from: dict, key: "size"),
            dts: uint64(from: dict, key: "dts"),
            cts: int64(from: dict, key: "cts"),
            wc: uint64(from: dict, key: "wc"),
            clockRate: uint32(from: dict, key: "clock_rate"),
            sequence: uint16(from: dict, key: "sequence"),
            packetPosition: dict["packet_position"] as? String
        )
    }

    // MARK: - JSON number helpers

    private static func uint64(from dict: [String: Any], key: String) -> UInt64? {
        (dict[key] as? UInt64) ?? (dict[key] as? Int).map(UInt64.init)
    }

    private static func uint32(from dict: [String: Any], key: String) -> UInt32? {
        (dict[key] as? UInt32) ?? (dict[key] as? Int).map(UInt32.init)
    }

    private static func uint16(from dict: [String: Any], key: String) -> UInt16? {
        (dict[key] as? UInt16) ?? (dict[key] as? Int).map(UInt16.init)
    }

    private static func int64(from dict: [String: Any], key: String) -> Int64? {
        (dict[key] as? Int64) ?? (dict[key] as? Int).map(Int64.init)
    }
}
