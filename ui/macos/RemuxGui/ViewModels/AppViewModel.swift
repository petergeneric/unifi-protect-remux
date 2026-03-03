import Foundation
import AppKit

@Observable
@MainActor
final class AppViewModel {

    // MARK: - Navigation
    var currentView: Int = 0

    // MARK: - File queue
    var files: [QueuedFile] = []
    var selectedFile: QueuedFile?

    // MARK: - Processing
    var isProcessing = false
    var isDiagnosticsProcessing = false
    var progressText = ""
    var hasProgressInfo = false
    var progressPercent: Double = 0

    // MARK: - Settings
    var withAudio = true
    var withVideo = true
    var forceRate: UInt32 = 0
    var fastStart = false
    var outputFolder = RemuxConfig.defaultOutputFolder
    var mp4Output = true
    var videoTrack: UInt16 = 0

    // MARK: - Log
    var logLines: [LogEntry] = []
    var logFilterLevel = "All"
    var logSearchText = ""
    var logFileFilter: Int?

    var filteredLogLines: [LogEntry] {
        let filterLevel = logFilterLevel.lowercased()
        let searchText = logSearchText.trimmingCharacters(in: .whitespaces)

        return logLines.filter { entry in
            if let fileFilter = logFileFilter, entry.fileIndex != fileFilter {
                return false
            }
            if filterLevel != "all" && entry.level.lowercased() != filterLevel {
                return false
            }
            if !searchText.isEmpty &&
               !entry.message.localizedCaseInsensitiveContains(searchText) {
                return false
            }
            return true
        }
    }

    var infoCount: Int { logLines.filter { $0.level.lowercased() != "error" && $0.level.lowercased() != "warn" }.count }
    var warnCount: Int { logLines.filter { $0.level.lowercased() == "warn" }.count }
    var errorCount: Int { logLines.filter { $0.level.lowercased() == "error" }.count }

    var logFileFilterLabel: String? {
        guard let idx = logFileFilter, idx < files.count else { return nil }
        return files[idx].fileName
    }

    // MARK: - Cameras
    var cameras: [CameraEntry] = []
    var hasUnsavedCameraChanges = false
    var cameraSaveLabel = "Save"

    // MARK: - Computed
    var isBusy: Bool { isProcessing || isDiagnosticsProcessing }

    // MARK: - Cancellation
    private var processingTask: Task<Void, Never>?

    // MARK: - UBV Info
    var ubvInfoPath: String = ""
    var ubvInfoFileName: String = ""
    var ubvInfoJSON: String = ""
    var showUbvInfo = false

    // MARK: - Init
    init() {
        loadCameras()
    }

    // MARK: - File management

    /// Add files to the queue. Returns paths that are low-res and need confirmation.
    func addFiles(_ urls: [URL]) -> [URL] {
        var warnedPaths: [URL] = []

        for url in urls {
            let path = url.path
            let lower = path.lowercased()
            guard lower.hasSuffix(".ubv") || lower.hasSuffix(".ubv.gz") else { continue }
            guard !files.contains(where: { $0.path == path }) else { continue }

            let filename = url.lastPathComponent
            if RemuxFFI.isLowResFilename(filename) {
                warnedPaths.append(url)
            } else {
                let qf = QueuedFile(path: path)
                ensureCameraEntry(qf.macAddress)
                qf.cameraName = lookupCameraName(qf.macAddress)
                files.append(qf)
            }
        }

        if selectedFile == nil {
            selectedFile = files.first
        }
        return warnedPaths
    }

    /// Add previously-warned paths after user confirmation.
    func addWarnedFiles(_ urls: [URL]) {
        for url in urls {
            let qf = QueuedFile(path: url.path)
            ensureCameraEntry(qf.macAddress)
            qf.cameraName = lookupCameraName(qf.macAddress)
            files.append(qf)
        }
        if selectedFile == nil {
            selectedFile = files.first
        }
    }

    func removeFile(_ file: QueuedFile) {
        guard !isBusy else { return }
        guard let idx = files.firstIndex(where: { $0.id == file.id }) else { return }
        files.remove(at: idx)
        if selectedFile?.id == file.id {
            selectedFile = files.isEmpty ? nil : files[min(idx, files.count - 1)]
        }
    }

    // MARK: - Config

    private func buildConfig() -> RemuxConfig {
        RemuxConfig(
            withAudio: withAudio,
            withVideo: withVideo,
            forceRate: forceRate,
            fastStart: fastStart,
            outputFolder: outputFolder,
            mp4: mp4Output,
            videoTrack: videoTrack
        )
    }

    private func sanitizeBaseName(_ name: String?) -> String? {
        guard let name, !name.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }
        let invalidChars = CharacterSet(charactersIn: "/\\:*?\"<>|")
        let sanitized = name.unicodeScalars
            .filter { !invalidChars.contains($0) }
            .map { Character($0) }
        let result = String(sanitized).trimmingCharacters(in: .whitespaces)
        return result.isEmpty ? nil : result
    }

    // MARK: - Processing

    func startAll() {
        guard !isBusy, !files.isEmpty else { return }

        logLines.removeAll()
        progressText = ""
        hasProgressInfo = false
        progressPercent = 0

        for f in files {
            f.status = .pending
            f.outputFiles.removeAll()
            f.error = nil
        }

        isProcessing = true
        let config = buildConfig()
        let filePaths = files.map(\.path)
        let baseNames = files.map { sanitizeBaseName($0.cameraName) }

        processingTask = Task.detached { [weak self] in
            RemuxFFI.initialize()

            for i in 0..<filePaths.count {
                if Task.isCancelled { break }

                var fileConfig = config
                fileConfig.baseName = baseNames[i]

                RemuxFFI.processFile(
                    path: filePaths[i],
                    config: fileConfig,
                    fileIndex: Int32(i)
                ) { [weak self] json, idx in
                    Task { @MainActor [weak self] in
                        self?.handleProgressJSON(fileIndex: Int(idx), json: json)
                    }
                }
            }

            await MainActor.run { [weak self] in
                self?.isProcessing = false
                self?.processingTask = nil
            }
        }
    }

    func convertFile(_ file: QueuedFile) {
        guard !isBusy else { return }
        guard let fileIndex = files.firstIndex(where: { $0.id == file.id }) else { return }

        file.status = .pending
        file.outputFiles.removeAll()
        file.error = nil

        logLines.removeAll()
        progressText = ""
        hasProgressInfo = false
        progressPercent = 0

        isProcessing = true
        var config = buildConfig()
        config.baseName = sanitizeBaseName(file.cameraName)
        let path = file.path
        let idx = Int32(fileIndex)

        processingTask = Task.detached { [weak self] in
            RemuxFFI.initialize()
            RemuxFFI.processFile(path: path, config: config, fileIndex: idx) { [weak self] json, fileIdx in
                Task { @MainActor [weak self] in
                    self?.handleProgressJSON(fileIndex: Int(fileIdx), json: json)
                }
            }
            await MainActor.run { [weak self] in
                self?.isProcessing = false
                self?.processingTask = nil
            }
        }
    }

    func cancel() {
        processingTask?.cancel()
    }

    func runDiagnostics(_ file: QueuedFile) {
        guard !isBusy else { return }
        guard let fileIndex = files.firstIndex(where: { $0.id == file.id }) else { return }

        isDiagnosticsProcessing = true
        file.status = .processing
        let path = file.path
        let fileName = file.fileName

        Task.detached { [weak self] in
            RemuxFFI.initialize()
            let (json, error) = RemuxFFI.ubvInfo(path: path)

            await MainActor.run { [weak self] in
                guard let self else { return }
                if let json {
                    if fileIndex < self.files.count {
                        self.files[fileIndex].status = .completed
                    }
                    self.ubvInfoPath = path
                    self.ubvInfoFileName = fileName
                    self.ubvInfoJSON = json
                    self.showUbvInfo = true
                } else {
                    if fileIndex < self.files.count {
                        self.files[fileIndex].status = .failed
                        self.files[fileIndex].error = error
                    }
                    self.logLines.append(LogEntry(level: "error", message: error ?? "Unknown error", fileIndex: fileIndex))
                }
                self.isDiagnosticsProcessing = false
            }
        }
    }

    // MARK: - Progress handling

    private func handleProgressJSON(fileIndex: Int, json: String) {
        guard let data = json.data(using: .utf8),
              let evt = try? JSONDecoder().decode(ProgressEvent.self, from: data) else {
            return
        }
        handleProgressEvent(fileIndex: fileIndex, evt: evt)
    }

    private func handleProgressEvent(fileIndex: Int, evt: ProgressEvent) {
        switch evt.type {
        case "log":
            logLines.append(LogEntry(level: evt.level ?? "info", message: evt.message ?? "", fileIndex: fileIndex))

        case "file_started":
            if fileIndex < files.count {
                files[fileIndex].status = .processing
            }
            progressText = "File \(fileIndex + 1) of \(files.count)"

        case "partitions_found":
            if fileIndex < files.count {
                files[fileIndex].partitionCount = evt.count
            }
            hasProgressInfo = true
            progressPercent = 0
            logLines.append(LogEntry(level: "info", message: "Found \(evt.count ?? 0) partition(s)", fileIndex: fileIndex))

        case "partition_started":
            if let total = evt.total, total > 0 {
                let partIdx = evt.index ?? 0
                progressPercent = Double(partIdx) / Double(total) * 100
                progressText = "File \(fileIndex + 1) of \(files.count) \u{2014} partition \(partIdx + 1)/\(total)"
            }
            logLines.append(LogEntry(level: "info", message: "Processing partition \((evt.index ?? 0) + 1)/\(evt.total ?? 0)", fileIndex: fileIndex))

        case "output_generated":
            if let path = evt.path {
                if fileIndex < files.count {
                    let qf = files[fileIndex]
                    qf.outputFiles.append(path)

                    // Extract thumbnail from first MP4
                    if qf.thumbnail == nil && path.lowercased().hasSuffix(".mp4") {
                        let mp4Path = path
                        let fileIdx = fileIndex
                        Task.detached { [weak self] in
                            let image = Self.extractThumbnailImage(mp4Path: mp4Path)
                            if let image {
                                await MainActor.run { [weak self] in
                                    guard let self, fileIdx < self.files.count else { return }
                                    self.files[fileIdx].thumbnail = image
                                }
                            }
                        }
                    }
                }
            }

        case "partition_error":
            logLines.append(LogEntry(level: "error", message: "Partition #\(evt.index ?? 0): \(evt.error ?? "")", fileIndex: fileIndex))

        case "file_completed":
            if fileIndex < files.count {
                if evt.errors == nil || evt.errors?.isEmpty == true {
                    files[fileIndex].status = .completed
                } else {
                    files[fileIndex].status = .failed
                    files[fileIndex].error = evt.errors?.joined(separator: "; ")
                }
            }
            progressPercent = 100

        default:
            break
        }
    }

    private static nonisolated func extractThumbnailImage(mp4Path: String) -> NSImage? {
        let thumbPath = NSTemporaryDirectory() + "remuxgui_\(UUID().uuidString).jpg"
        let error = RemuxFFI.extractThumbnail(mp4Path: mp4Path, outputPath: thumbPath)
        guard error == nil else { return nil }

        guard let data = try? Data(contentsOf: URL(fileURLWithPath: thumbPath)) else { return nil }
        try? FileManager.default.removeItem(atPath: thumbPath)

        return NSImage(data: data)
    }

    // MARK: - Log

    func clearLog() {
        logLines.removeAll()
    }

    func viewFileLog() {
        guard let file = selectedFile,
              let idx = files.firstIndex(where: { $0.id == file.id }) else { return }
        logFileFilter = idx
        currentView = 2
    }

    func clearLogFileFilter() {
        logFileFilter = nil
    }

    // MARK: - Camera management

    func removeCamera(_ entry: CameraEntry) {
        cameras.removeAll { $0.id == entry.id }
        saveCameras()
    }

    func saveCameras() {
        RemuxFFI.saveCameras(cameras)
        hasUnsavedCameraChanges = false
    }

    func saveCamerasExplicit() {
        saveCameras()
        cameraSaveLabel = "Saved!"
        Task {
            try? await Task.sleep(for: .milliseconds(1500))
            cameraSaveLabel = "Save"
        }
    }

    func loadCameras() {
        cameras = RemuxFFI.loadCameras()
    }

    func refreshAllCameraNames() {
        for file in files {
            file.cameraName = lookupCameraName(file.macAddress)
        }
    }

    private func ensureCameraEntry(_ mac: String?) {
        guard let mac, !mac.isEmpty else { return }
        if cameras.contains(where: { $0.macAddress.caseInsensitiveCompare(mac) == .orderedSame }) {
            return
        }
        cameras.append(CameraEntry(macAddress: mac))
    }

    private func lookupCameraName(_ mac: String?) -> String? {
        guard let mac, !mac.isEmpty else { return nil }
        for cam in cameras {
            if cam.macAddress.caseInsensitiveCompare(mac) == .orderedSame,
               !cam.friendlyName.trimmingCharacters(in: .whitespaces).isEmpty {
                return cam.friendlyName
            }
        }
        return nil
    }
}
