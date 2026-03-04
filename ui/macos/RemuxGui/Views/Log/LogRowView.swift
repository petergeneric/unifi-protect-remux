import SwiftUI

struct LogRowView: View {
    let entry: LogEntry

    private var levelColor: Color {
        switch entry.level {
        case .error: .logError
        case .warn: .logWarn
        case .info: .logInfo
        }
    }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text(entry.timestampLabel)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.tertiary)
                .frame(width: 62, alignment: .leading)

            Text(entry.level.rawValue.uppercased())
                .font(.system(size: 9, weight: .medium, design: .monospaced))
                .foregroundStyle(.white)
                .padding(.horizontal, 4)
                .padding(.vertical, 1)
                .background(levelColor.opacity(entry.level == .info ? 0.4 : 0.8))
                .clipShape(RoundedRectangle(cornerRadius: 3))
                .frame(width: 46, alignment: .leading)

            Text(entry.message)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.primary)
                .textSelection(.enabled)
        }
        .padding(.vertical, 1)
    }
}
