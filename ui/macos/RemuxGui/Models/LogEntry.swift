import Foundation

struct LogEntry: Identifiable {
    let id = UUID()
    let level: String
    let message: String
    let timestamp: Date
    let fileIndex: Int?

    var timestampLabel: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        return formatter.string(from: timestamp)
    }

    init(level: String, message: String, fileIndex: Int? = nil) {
        self.level = level
        self.message = message
        self.timestamp = Date()
        self.fileIndex = fileIndex
    }
}
