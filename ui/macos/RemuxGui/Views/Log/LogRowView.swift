import SwiftUI

struct LogRowView: View {
    let entry: LogEntry

    private var levelColor: Color {
        switch entry.level.lowercased() {
        case "error": .logError
        case "warn": .logWarn
        default: .logInfo
        }
    }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Text(entry.timestampLabel)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 60, alignment: .leading)

            Text("[\(entry.level)]")
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(levelColor)
                .frame(width: 50, alignment: .leading)

            Text(entry.message)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.primary)
                .textSelection(.enabled)
        }
        .padding(.vertical, 1)
    }
}
