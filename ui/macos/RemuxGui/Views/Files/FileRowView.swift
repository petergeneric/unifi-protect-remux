import SwiftUI

struct FileRowView: View {
    @Environment(AppViewModel.self) private var vm
    let file: QueuedFile
    @State private var isHovered = false

    private var statusColor: Color {
        switch file.status {
        case .pending: .statusPending
        case .processing: .statusProcessing
        case .completed: .statusCompleted
        case .failed: .statusFailed
        }
    }

    var body: some View {
        HStack(spacing: 8) {
            // Status indicator
            Circle()
                .fill(statusColor)
                .frame(width: 8, height: 8)

            VStack(alignment: .leading, spacing: 2) {
                Text(file.fileName)
                    .font(.system(.body, design: .monospaced))
                    .lineLimit(1)

                HStack(spacing: 8) {
                    if let cameraName = file.cameraName {
                        Text(cameraName)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    if let ts = file.fileTimestampLabel {
                        Text(ts)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    if let size = file.fileSizeLabel {
                        Text(size)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            Spacer()

            Text(file.statusLabel)
                .font(.caption)
                .foregroundStyle(statusColor)

            if isHovered && !vm.isBusy {
                Button {
                    vm.removeFile(file)
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
        }
        .onHover { isHovered = $0 }
    }
}
