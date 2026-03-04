import SwiftUI

struct FileDetailView: View {
    @Environment(AppViewModel.self) private var vm
    let file: QueuedFile

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                // Thumbnail
                if let thumbnail = file.thumbnail {
                    Image(nsImage: thumbnail)
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .frame(maxWidth: .infinity)
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                        .shadow(color: .black.opacity(0.1), radius: 2, y: 1)
                        .accessibilityLabel("Video thumbnail for \(file.fileName)")
                }

                // Info section
                GroupBox("Details") {
                    Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 4) {
                        DetailGridRow(label: "Status", value: file.statusLabel, valueColor: file.status.color)

                        if let mac = file.macAddress {
                            DetailGridRow(label: "MAC", value: mac)
                        }
                        if let ts = file.fileTimestampLabel {
                            DetailGridRow(label: "Timestamp", value: ts)
                        }
                        if let size = file.fileSizeLabel {
                            DetailGridRow(label: "Size", value: size)
                        }
                        if let count = file.partitionCount {
                            DetailGridRow(label: "Partitions", value: "\(count)")
                        }
                        if let outSize = file.outputSizeLabel {
                            DetailGridRow(label: "Output Size", value: outSize)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }

                // Output files
                if !file.outputFiles.isEmpty {
                    GroupBox("Output Files") {
                        VStack(alignment: .leading, spacing: 4) {
                            ForEach(file.outputFiles, id: \.self) { path in
                                Button {
                                    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
                                } label: {
                                    HStack(spacing: 4) {
                                        Image(systemName: "doc.fill")
                                            .font(.caption2)
                                        Text((path as NSString).lastPathComponent)
                                            .lineLimit(1)
                                            .truncationMode(.middle)
                                        Spacer()
                                        Image(systemName: "arrow.up.forward.square")
                                            .font(.caption2)
                                    }
                                    .font(.caption)
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                                .foregroundStyle(Color.accentColor)
                                .help("Reveal in Finder")
                                .accessibilityLabel("Reveal \(URL(fileURLWithPath: path).lastPathComponent) in Finder")
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }

                // Error
                if let error = file.error {
                    GroupBox {
                        Label(error, systemImage: "exclamationmark.triangle.fill")
                            .font(.caption)
                            .foregroundStyle(.red)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel("Error: \(error)")
                }

                // Action buttons
                actionButtons
            }
            .padding(12)
        }
        .background(Color(nsColor: .controlBackgroundColor))
    }

    private var actionButtons: some View {
        HStack(spacing: 8) {
            Button("Convert") {
                vm.convertFile(file)
            }
            .disabled(vm.isBusy)

            Button("Info") {
                vm.runDiagnostics(file)
            }
            .disabled(vm.isBusy)
            .help("Inspect UBV structure")

            Button("View Log") {
                vm.viewFileLog()
            }
            .help("Show log filtered to this file")
        }
    }
}

private struct DetailGridRow: View {
    let label: String
    let value: String
    var valueColor: Color? = nil

    var body: some View {
        GridRow {
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
                .gridColumnAlignment(.trailing)
            Text(value)
                .font(.caption)
                .foregroundStyle(valueColor ?? .primary)
                .textSelection(.enabled)
        }
    }
}
