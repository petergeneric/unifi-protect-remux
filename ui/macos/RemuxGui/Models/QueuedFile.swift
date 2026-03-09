import Foundation
import AppKit

@Observable
final class QueuedFile: Identifiable {
    private static let timestampFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd HH:mm:ss"
        return f
    }()

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
    var outputSizeLabel: String?

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

    init(path: String) {
        let url = URL(fileURLWithPath: path)
        self.path = path
        self.fileName = url.lastPathComponent
        self.macAddress = RemuxFFI.extractMAC(filename: url.lastPathComponent)

        if let tsString = RemuxFFI.extractTimestamp(filename: url.lastPathComponent),
           let millis = Int64(tsString) {
            let date = Date(timeIntervalSince1970: Double(millis) / 1000.0)
            self.fileTimestamp = date
            self.fileTimestampLabel = Self.timestampFormatter.string(from: date)
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

    func updateOutputSize() {
        var total: Int64 = 0
        for file in outputFiles {
            if let attrs = try? FileManager.default.attributesOfItem(atPath: file),
               let size = attrs[.size] as? Int64 {
                total += size
            }
        }
        outputSizeLabel = total > 0 ? Self.formatFileSize(total) : nil
    }

    private nonisolated(unsafe) static let fileSizeFormatter: ByteCountFormatter = {
        let f = ByteCountFormatter()
        f.countStyle = .binary
        return f
    }()

    static func formatFileSize(_ bytes: Int64) -> String {
        fileSizeFormatter.string(fromByteCount: bytes)
    }
}
