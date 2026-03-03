import Foundation
import AppKit

@Observable
final class QueuedFile: Identifiable {
    let id = UUID()
    let path: String
    let fileName: String
    let fileSize: Int64?
    let fileSizeLabel: String?
    let macAddress: String?
    let fileTimestamp: Date?
    let fileTimestampLabel: String?

    var status: FileStatus = .pending
    var error: String?
    var partitionCount: Int?
    var cameraName: String?
    var thumbnail: NSImage?
    var outputFiles: [String] = []

    var statusLabel: String {
        switch status {
        case .pending:
            "Pending"
        case .processing:
            "Processing..."
        case .completed where outputFiles.count == 1:
            "Done (1 file)"
        case .completed where outputFiles.count > 1:
            "Done (\(outputFiles.count) files)"
        case .completed:
            "Done"
        case .failed:
            "Failed"
        }
    }

    var outputSizeLabel: String? {
        var total: Int64 = 0
        for file in outputFiles {
            if let attrs = try? FileManager.default.attributesOfItem(atPath: file),
               let size = attrs[.size] as? Int64 {
                total += size
            }
        }
        return total > 0 ? Self.formatFileSize(total) : nil
    }

    init(path: String) {
        self.path = path
        self.fileName = (path as NSString).lastPathComponent
        self.macAddress = RemuxFFI.extractMAC(filename: (path as NSString).lastPathComponent)

        if let tsString = RemuxFFI.extractTimestamp(filename: (path as NSString).lastPathComponent),
           let millis = Int64(tsString) {
            let date = Date(timeIntervalSince1970: Double(millis) / 1000.0)
            self.fileTimestamp = date
            let formatter = DateFormatter()
            formatter.dateFormat = "yyyy-MM-dd HH:mm:ss"
            self.fileTimestampLabel = formatter.string(from: date)
        } else {
            self.fileTimestamp = nil
            self.fileTimestampLabel = nil
        }

        if let attrs = try? FileManager.default.attributesOfItem(atPath: path),
           let size = attrs[.size] as? Int64 {
            self.fileSize = size
            self.fileSizeLabel = Self.formatFileSize(size)
        } else {
            self.fileSize = nil
            self.fileSizeLabel = nil
        }
    }

    static func formatFileSize(_ bytes: Int64) -> String {
        if bytes < 1024 {
            return "\(bytes) B"
        } else if bytes < 1024 * 1024 {
            return "\(bytes / 1024) KB"
        } else if bytes < 1024 * 1024 * 1024 {
            return String(format: "%.0f MB", Double(bytes) / (1024.0 * 1024.0))
        } else {
            return String(format: "%.1f GB", Double(bytes) / (1024.0 * 1024.0 * 1024.0))
        }
    }
}
