import SwiftUI

struct FileDetailView: View {
    @Environment(AppViewModel.self) private var vm
    let file: QueuedFile

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                // Thumbnail
                if let thumbnail = file.thumbnail {
                    Image(nsImage: thumbnail)
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .frame(maxWidth: .infinity)
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                }

                // Info grid
                infoSection

                // Output files
                if !file.outputFiles.isEmpty {
                    outputSection
                }

                // Error
                if let error = file.error {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .padding(8)
                        .background(Color.red.opacity(0.1))
                        .clipShape(RoundedRectangle(cornerRadius: 4))
                }

                // Action buttons
                actionButtons
            }
            .padding()
        }
        .background(Color(nsColor: .controlBackgroundColor))
    }

    private var infoSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("File Details")
                .font(.headline)

            DetailRow(label: "Status", value: file.statusLabel)

            if let mac = file.macAddress {
                DetailRow(label: "MAC", value: mac)
            }
            if let ts = file.fileTimestampLabel {
                DetailRow(label: "Timestamp", value: ts)
            }
            if let size = file.fileSizeLabel {
                DetailRow(label: "Size", value: size)
            }
            if let count = file.partitionCount {
                DetailRow(label: "Partitions", value: "\(count)")
            }
            if let outSize = file.outputSizeLabel {
                DetailRow(label: "Output Size", value: outSize)
            }
        }
    }

    private var outputSection: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Output Files")
                .font(.headline)

            ForEach(file.outputFiles, id: \.self) { path in
                Button {
                    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
                } label: {
                    HStack {
                        Image(systemName: "doc")
                        Text((path as NSString).lastPathComponent)
                            .lineLimit(1)
                        Spacer()
                        Image(systemName: "arrow.up.right.square")
                            .font(.caption)
                    }
                    .font(.caption)
                }
                .buttonStyle(.plain)
                .foregroundStyle(Color.accentColor)
            }
        }
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

            Button("View Log") {
                vm.viewFileLog()
            }
        }
    }
}

private struct DetailRow: View {
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label)
                .foregroundStyle(.secondary)
                .frame(width: 80, alignment: .trailing)
            Text(value)
        }
        .font(.caption)
    }
}
