import SwiftUI

struct FileRowView: View {
    @Environment(AppViewModel.self) private var vm
    let file: QueuedFile
    @State private var isHovered = false

    var body: some View {
        HStack(spacing: 8) {
            // Status indicator (decorative — statusLabel conveys this to VoiceOver)
            Circle()
                .fill(file.status.color)
                .frame(width: 7, height: 7)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 2) {
                Text(file.fileName)
                    .font(.system(.body, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)

                HStack(spacing: 6) {
                    if let cameraName = file.cameraName {
                        Label(cameraName, systemImage: "video")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    if let ts = file.fileTimestampLabel {
                        Text(ts)
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }
                    if let size = file.fileSizeLabel {
                        Text(size)
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                            .monospacedDigit()
                    }
                }
            }

            Spacer()

            if file.status == .processing {
                ProgressView()
                    .controlSize(.small)
            }

            Text(file.statusLabel)
                .font(.caption)
                .foregroundStyle(file.status.color)

            Button {
                vm.removeFile(file)
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .foregroundStyle(.tertiary)
            }
            .buttonStyle(.plain)
            .opacity(isHovered && !vm.isBusy ? 1 : 0)
            .disabled(vm.isBusy)
            .help("Remove from queue")
            .accessibilityLabel("Remove \(file.fileName)")
        }
        .padding(.vertical, 2)
        .onHover { isHovered = $0 }
    }
}
