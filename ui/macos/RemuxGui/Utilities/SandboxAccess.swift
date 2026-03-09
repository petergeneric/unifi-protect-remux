import Foundation
import AppKit

enum SandboxAccess {
    private static let outputFolderBookmarkKey = "outputFolderBookmark"
    private static let sourceDirBookmarksKey = "sourceDirBookmarks"

    // MARK: - Output folder bookmark

    static func saveOutputFolderBookmark(for url: URL) {
        guard let data = try? url.bookmarkData(
            options: .withSecurityScope,
            includingResourceValuesForKeys: nil,
            relativeTo: nil
        ) else { return }
        UserDefaults.standard.set(data, forKey: outputFolderBookmarkKey)
    }

    static func loadOutputFolderURL() -> URL? {
        guard let data = UserDefaults.standard.data(forKey: outputFolderBookmarkKey) else { return nil }
        var isStale = false
        guard let url = try? URL(
            resolvingBookmarkData: data,
            options: .withSecurityScope,
            relativeTo: nil,
            bookmarkDataIsStale: &isStale
        ) else { return nil }
        if isStale {
            saveOutputFolderBookmark(for: url)
        }
        return url
    }

    static func clearOutputFolderBookmark() {
        UserDefaults.standard.removeObject(forKey: outputFolderBookmarkKey)
    }

    // MARK: - Source directory bookmarks

    /// Save a security-scoped bookmark for a source directory so we can write MP4s next to UBVs.
    static func saveSourceDirBookmark(for url: URL) {
        var bookmarks = loadSourceDirBookmarksRaw()
        guard let data = try? url.bookmarkData(
            options: .withSecurityScope,
            includingResourceValuesForKeys: nil,
            relativeTo: nil
        ) else { return }
        bookmarks[url.path] = data
        UserDefaults.standard.set(bookmarks, forKey: sourceDirBookmarksKey)
    }

    /// Attempt to restore access to a previously-bookmarked source directory.
    /// Returns true if access was granted.
    static func restoreSourceDirAccess(for directoryPath: String) -> Bool {
        let bookmarks = loadSourceDirBookmarksRaw()
        guard let data = bookmarks[directoryPath] else { return false }
        var isStale = false
        guard let url = try? URL(
            resolvingBookmarkData: data,
            options: .withSecurityScope,
            relativeTo: nil,
            bookmarkDataIsStale: &isStale
        ) else { return false }
        let ok = url.startAccessingSecurityScopedResource()
        if ok && isStale {
            saveSourceDirBookmark(for: url)
        }
        return ok
    }

    /// Present an NSOpenPanel asking the user to grant access to the given directory.
    /// Returns the URL the user selected, or nil if they cancelled.
    static func requestSourceDirAccess(for directoryURL: URL) -> URL? {
        let panel = NSOpenPanel()
        panel.message = "UBV Remux needs access to this folder to write output files next to your source files."
        panel.prompt = "Grant Access"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.directoryURL = directoryURL
        guard panel.runModal() == .OK, let url = panel.url else { return nil }
        _ = url.startAccessingSecurityScopedResource()
        saveSourceDirBookmark(for: url)
        return url
    }

    private static func loadSourceDirBookmarksRaw() -> [String: Data] {
        UserDefaults.standard.dictionary(forKey: sourceDirBookmarksKey) as? [String: Data] ?? [:]
    }
}
