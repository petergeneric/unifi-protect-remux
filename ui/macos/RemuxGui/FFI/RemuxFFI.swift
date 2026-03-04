import Foundation

/// Swift wrapper around the C FFI functions exposed by `libremux_ffi`.
enum RemuxFFI {

    // MARK: - Private helpers

    private struct VersionInfo: Decodable {
        let version: String
        let gitCommit: String
        enum CodingKeys: String, CodingKey {
            case version
            case gitCommit = "git_commit"
        }
    }

    private struct ValidationResult: Decodable {
        let valid: Bool
        let error: String?
    }

    private struct CamerasPayload: Decodable {
        let cameras: [CameraJSON]
        struct CameraJSON: Decodable {
            let mac: String
            let name: String?
        }
    }

    private struct DiagnosticsResult: Decodable {
        let outputPath: String
        enum CodingKeys: String, CodingKey {
            case outputPath = "output_path"
        }
    }

    private static let decoder = JSONDecoder()

    // MARK: - Lifecycle

    /// Initialise FFmpeg. Safe to call multiple times.
    static func initialize() {
        remux_init()
    }

    // MARK: - Metadata

    /// Return version info (version string and git commit).
    static func version() -> (version: String, gitCommit: String) {
        guard let ptr = remux_version() else {
            return ("unknown", "")
        }
        defer { remux_free_string(ptr) }
        let json = String(cString: ptr)
        guard let data = json.data(using: .utf8),
              let info = try? decoder.decode(VersionInfo.self, from: data) else {
            return ("unknown", "")
        }
        return (info.version, info.gitCommit)
    }

    /// Return third-party license information.
    static func licenses() -> [LicenseEntry] {
        guard let ptr = remux_licenses() else { return [] }
        defer { remux_free_string(ptr) }
        let json = String(cString: ptr)
        guard let data = json.data(using: .utf8) else { return [] }
        return (try? JSONDecoder().decode([LicenseEntry].self, from: data)) ?? []
    }

    // MARK: - Config validation

    /// Validate a remux configuration. Returns nil on success, or an error string.
    static func validateConfig(_ config: RemuxConfig) -> String? {
        guard let jsonData = try? JSONEncoder().encode(config),
              let jsonString = String(data: jsonData, encoding: .utf8) else {
            return "Failed to encode config"
        }
        guard let ptr = jsonString.withCString({ remux_validate_config($0) }) else {
            return "Internal error: null result"
        }
        defer { remux_free_string(ptr) }
        let resultJson = String(cString: ptr)
        guard let data = resultJson.data(using: .utf8),
              let result = try? decoder.decode(ValidationResult.self, from: data) else {
            return "Failed to parse validation result"
        }
        return result.valid ? nil : (result.error ?? "Unknown validation error")
    }

    // MARK: - Processing

    /// Process a `.ubv` file with a context-based callback for progress events.
    ///
    /// The `onProgress` closure receives (jsonEventString, fileIndex) for each
    /// progress event emitted by the Rust layer.
    @discardableResult
    static func processFile(
        path: String,
        config: RemuxConfig,
        fileIndex: Int32,
        onProgress: @escaping (String, Int32) -> Void
    ) -> (resultJSON: String?, error: String?) {
        guard let jsonData = try? JSONEncoder().encode(config),
              let configJSON = String(data: jsonData, encoding: .utf8) else {
            return (nil, "Failed to encode config")
        }

        // Box the closure and pass it as an opaque context pointer
        typealias Callback = (String, Int32) -> Void
        let boxed = Unmanaged.passRetained(Box(onProgress))
        let context = boxed.toOpaque()

        let cCallback: @convention(c) (UnsafePointer<CChar>?, Int32, UnsafeMutableRawPointer?) -> Void = {
            jsonPtr, idx, ctx in
            guard let jsonPtr, let ctx else { return }
            let json = String(cString: jsonPtr)
            let cb = Unmanaged<Box<Callback>>.fromOpaque(ctx).takeUnretainedValue()
            cb.wrappedValue(json, idx)
        }

        var errorPtr: UnsafeMutablePointer<CChar>?
        let resultPtr = path.withCString { pathC in
            configJSON.withCString { configC in
                remux_process_file_ctx(pathC, configC, cCallback, fileIndex, context, &errorPtr)
            }
        }

        // Release the boxed closure
        boxed.release()

        let error: String? = errorPtr.map { p in
            defer { remux_free_string(p) }
            return String(cString: p)
        }
        let result: String? = resultPtr.map { p in
            defer { remux_free_string(p) }
            return String(cString: p)
        }
        return (result, error)
    }

    // MARK: - Filename helpers

    /// Extract the 12-char uppercase hex MAC from a UBV filename.
    static func extractMAC(filename: String) -> String? {
        guard let ptr = filename.withCString({ remux_extract_mac($0) }) else { return nil }
        defer { remux_free_string(ptr) }
        return String(cString: ptr)
    }

    /// Extract the unix-milliseconds timestamp string from a UBV filename.
    static func extractTimestamp(filename: String) -> String? {
        guard let ptr = filename.withCString({ remux_extract_timestamp($0) }) else { return nil }
        defer { remux_free_string(ptr) }
        return String(cString: ptr)
    }

    /// Check whether a filename indicates a low-res recording.
    static func isLowResFilename(_ filename: String) -> Bool {
        filename.withCString { remux_is_low_res_filename($0) } != 0
    }

    // MARK: - Camera persistence

    /// Load cameras from the platform-specific data directory.
    static func loadCameras() -> [CameraEntry] {
        guard let ptr = remux_load_cameras() else { return [] }
        defer { remux_free_string(ptr) }
        let json = String(cString: ptr)
        guard let data = json.data(using: .utf8),
              let payload = try? decoder.decode(CamerasPayload.self, from: data) else {
            return []
        }
        return payload.cameras.map {
            CameraEntry(macAddress: $0.mac, friendlyName: $0.name ?? "")
        }
    }

    /// Save cameras to the platform-specific data directory.
    @discardableResult
    static func saveCameras(_ cameras: [CameraEntry]) -> String? {
        let entries = cameras
            .filter { !$0.friendlyName.trimmingCharacters(in: .whitespaces).isEmpty }
            .map { ["mac": $0.macAddress, "name": $0.friendlyName] }
        let payload: [String: Any] = ["cameras": entries]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let jsonString = String(data: data, encoding: .utf8) else {
            return "Failed to encode cameras JSON"
        }
        var errorPtr: UnsafeMutablePointer<CChar>?
        let ret = jsonString.withCString { remux_save_cameras($0, &errorPtr) }
        if ret != 0 {
            if let errorPtr {
                defer { remux_free_string(errorPtr) }
                return String(cString: errorPtr)
            }
            return "Unknown error saving cameras"
        }
        return nil
    }

    // MARK: - Thumbnail

    /// Extract a JPEG thumbnail from an MP4 file.
    /// Returns nil on success, or an error message.
    static func extractThumbnail(mp4Path: String, outputPath: String, maxWidth: UInt32 = 320) -> String? {
        var errorPtr: UnsafeMutablePointer<CChar>?
        let ret = mp4Path.withCString { mp4C in
            outputPath.withCString { outC in
                remux_extract_thumbnail(mp4C, outC, maxWidth, &errorPtr)
            }
        }
        if ret == 0 { return nil }
        if let errorPtr {
            defer { remux_free_string(errorPtr) }
            return String(cString: errorPtr)
        }
        return "Unknown error"
    }

    // MARK: - UBV Info

    /// Parse a `.ubv` file and return its structure as a JSON string.
    static func ubvInfo(path: String) -> (json: String?, error: String?) {
        var errorPtr: UnsafeMutablePointer<CChar>?
        let resultPtr = path.withCString { remux_ubv_info($0, &errorPtr) }
        let error: String? = errorPtr.map { p in
            defer { remux_free_string(p) }
            return String(cString: p)
        }
        let json: String? = resultPtr.map { p in
            defer { remux_free_string(p) }
            return String(cString: p)
        }
        return (json, error ?? (json == nil ? "Unknown error" : nil))
    }

    // MARK: - Diagnostics

    /// Produce a gzip-compressed diagnostics file. Returns the output path or error.
    static func produceDiagnostics(path: String) -> (outputPath: String?, error: String?) {
        var errorPtr: UnsafeMutablePointer<CChar>?
        let resultPtr = path.withCString { remux_produce_diagnostics($0, &errorPtr) }
        let error: String? = errorPtr.map { p in
            defer { remux_free_string(p) }
            return String(cString: p)
        }
        guard let resultPtr else { return (nil, error ?? "Unknown error") }
        defer { remux_free_string(resultPtr) }
        let resultJSON = String(cString: resultPtr)
        guard let data = resultJSON.data(using: .utf8),
              let result = try? decoder.decode(DiagnosticsResult.self, from: data) else {
            return (nil, error ?? "Failed to parse diagnostics result")
        }
        return (result.outputPath, nil)
    }
}

/// Helper to box a value for Unmanaged pointer passing.
private final class Box<T> {
    let wrappedValue: T
    init(_ value: T) { self.wrappedValue = value }
}
