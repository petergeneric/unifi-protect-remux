import Foundation

enum LogLevel: String {
    case info
    case warn
    case error

    init(raw: String) {
        self = LogLevel(rawValue: raw.lowercased()) ?? .info
    }
}

struct LogEntry: Identifiable {
    let id = UUID()
    let level: LogLevel
    let message: String
    let timestamp: Date
    let fileIndex: Int?

    var timestampLabel: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        return formatter.string(from: timestamp)
    }

    init(level: LogLevel, message: String, fileIndex: Int? = nil) {
        self.level = level
        self.message = message
        self.timestamp = Date()
        self.fileIndex = fileIndex
    }
}
