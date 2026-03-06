import SwiftUI
import zlib

struct UbvInfoView: View {
    let ubvPath: String
    let fileName: String
    let json: String

    @State private var nodes: [UbvInfoTreeNode] = []
    @State private var selectedNodeId: UUID?
    @State private var selectedNode: UbvInfoTreeNode?
    @State private var expandedPartitions: Set<UUID> = []

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                // Left pane: tree
                leftPane
                    .frame(width: 260)

                Divider()

                // Right pane: partition summary or entry table
                rightPane
            }
        }
        .frame(minWidth: 800, minHeight: 500)
        .onAppear {
            nodes = UbvInfoParser.parse(json: json)
            for node in nodes {
                expandedPartitions.insert(node.id)
            }
            let first = nodes.first
            selectedNodeId = first?.id
            selectedNode = first
        }
        .onChange(of: selectedNodeId) { _, newId in
            if let newId {
                selectedNode = findNode(id: newId, in: nodes)
            } else {
                selectedNode = nil
            }
        }
    }

    // MARK: - Left pane

    private var leftPane: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Structure")
                    .font(.headline)
                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)

            List(selection: $selectedNodeId) {
                ForEach(nodes) { partition in
                    DisclosureGroup(
                        isExpanded: Binding(
                            get: { expandedPartitions.contains(partition.id) },
                            set: { expanded in
                                if expanded {
                                    expandedPartitions.insert(partition.id)
                                } else {
                                    expandedPartitions.remove(partition.id)
                                }
                            }
                        )
                    ) {
                        ForEach(partition.children) { child in
                            Text(child.label)
                                .font(.system(size: 13))
                                .tag(child.id)
                        }
                    } label: {
                        Text(partition.label)
                            .font(.system(size: 13))
                            .tag(partition.id)
                    }
                }
            }
            .listStyle(.inset)

            Divider()

            Button("Save JSON\u{2026}", systemImage: "square.and.arrow.down") {
                saveJSON()
            }
            .controlSize(.small)
            .padding(8)
            .frame(maxWidth: .infinity)
        }
    }

    // MARK: - Right pane

    @ViewBuilder
    private var rightPane: some View {
        if let node = selectedNode {
            if node.isPartition, let header = node.header {
                partitionSummaryView(header)
            } else {
                entryTableView(node)
            }
        } else {
            Text("Select an item to view details")
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    // MARK: - Partition summary

    private func partitionSummaryView(_ header: PartitionHeaderInfo) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Partition \(header.index)")
                    .font(.title3.bold())

                // Header fields
                VStack(alignment: .leading, spacing: 4) {
                    Text("HEADER")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .padding(.bottom, 4)

                    headerRow("File Offset", value: header.fileOffset.map(String.init))
                    headerRow("DTS", value: header.dts.map(String.init))
                    headerRow("Clock Rate", value: header.clockRate.map(String.init))
                    headerRow("Format Code", value: header.formatCode.map { String(format: "0x%04X", $0) })
                }

                Divider()

                // Entry counts
                VStack(alignment: .leading, spacing: 4) {
                    Text("ENTRIES (\(header.totalEntries))")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .padding(.bottom, 4)

                    ForEach(header.entryCounts, id: \.label) { item in
                        HStack {
                            Text(item.label)
                                .font(.system(size: 12))
                            Spacer()
                            Text("\(item.count)")
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
            .padding(24)
            .frame(maxWidth: 600, alignment: .leading)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private func headerRow(_ label: String, value: String?) -> some View {
        HStack {
            Text(label)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .frame(width: 110, alignment: .leading)
            Text(value ?? "")
                .font(.system(size: 12, design: .monospaced))
        }
        .padding(.vertical, 2)
    }

    // MARK: - Entry table

    private func entryTableView(_ node: UbvInfoTreeNode) -> some View {
        let showKf = node.entries.contains(where: { $0.keyframe == true })
            && node.entries.contains(where: { $0.keyframe == false })

        return VStack(spacing: 0) {
            // Fixed header row (column labels are included in row a11y labels)
            entryHeaderRow(showKf: showKf)
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
                .background(Color(nsColor: .controlBackgroundColor))
                .accessibilityHidden(true)

            Divider()

            // Scrollable entry rows
            List(node.entries) { entry in
                entryRow(entry, showKf: showKf)
                    .listRowInsets(EdgeInsets(top: 0, leading: 8, bottom: 0, trailing: 0))
            }
            .listStyle(.plain)
        }
    }

    private func entryHeaderRow(showKf: Bool) -> some View {
        HStack(spacing: 0) {
            Group {
                headerCell("Type", width: 40)
                headerCell("TID", width: 40)
                if showKf { headerCell("KF", width: 28) }
                headerCell("Offset")
                headerCell("Size")
                headerCell("DTS")
            }
            Group {
                headerCell("CTS")
                headerCell("WC")
                headerCell("CR")
                headerCell("Seq", width: 40)
                headerCell("Pos", width: 60)
            }
        }
    }

    @ViewBuilder
    private func headerCell(_ title: String, width: CGFloat? = nil) -> some View {
        let text = Text(title)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(.secondary)
        if let width {
            text.frame(width: width, alignment: .leading)
        } else {
            text.frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func entryRow(_ entry: UbvInfoEntry, showKf: Bool) -> some View {
        HStack(spacing: 0) {
            Group {
                cell(entry.type, width: 40)
                cell(entry.trackId.map(String.init), width: 40)
                if showKf { cell(entry.keyframeLabel, width: 28) }
                cell(entry.offset.map(String.init))
                cell(entry.size.map(String.init))
                cell(entry.dts.map(String.init))
            }
            Group {
                cell(entry.cts.map(String.init))
                cell(entry.wc.map(String.init))
                cell(entry.clockRate.map(String.init))
                cell(entry.sequence.map(String.init), width: 40)
                cell(entry.packetPosition, width: 60)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(entryAccessibilityLabel(entry, showKf: showKf))
    }

    private func entryAccessibilityLabel(_ entry: UbvInfoEntry, showKf: Bool) -> String {
        var parts: [String] = ["Type \(entry.type)"]
        if let tid = entry.trackId { parts.append("Track \(tid)") }
        if showKf { parts.append(entry.keyframe == true ? "Keyframe" : "Non-keyframe") }
        if let offset = entry.offset { parts.append("Offset \(offset)") }
        if let size = entry.size { parts.append("Size \(size)") }
        if let dts = entry.dts { parts.append("DTS \(dts)") }
        if let cts = entry.cts { parts.append("CTS \(cts)") }
        if let wc = entry.wc { parts.append("WC \(wc)") }
        if let cr = entry.clockRate { parts.append("Clock rate \(cr)") }
        if let seq = entry.sequence { parts.append("Sequence \(seq)") }
        if let pos = entry.packetPosition { parts.append("Position \(pos)") }
        return parts.joined(separator: ", ")
    }

    @ViewBuilder
    private func cell(_ text: String?, width: CGFloat? = nil) -> some View {
        let content = Text(text ?? "")
            .font(.system(size: 12, design: .monospaced))
            .lineLimit(1)
            .truncationMode(.tail)
        if let width {
            content.frame(width: width, alignment: .leading)
        } else {
            content.frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    // MARK: - Helpers

    private func findNode(id: UUID, in nodes: [UbvInfoTreeNode]) -> UbvInfoTreeNode? {
        for node in nodes {
            if node.id == id { return node }
            if let found = findNode(id: id, in: node.children) { return found }
        }
        return nil
    }

    private func saveJSON() {
        guard !json.isEmpty, !ubvPath.isEmpty else { return }

        let outputPath = ubvPath + ".json.gz"
        guard let jsonData = json.data(using: .utf8) else { return }

        let gz = gzopen(outputPath, "wb")
        guard gz != nil else { return }
        defer { gzclose(gz) }
        jsonData.withUnsafeBytes { buf in
            _ = gzwrite(gz, buf.baseAddress, UInt32(buf.count))
        }

        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: outputPath)])
    }
}
