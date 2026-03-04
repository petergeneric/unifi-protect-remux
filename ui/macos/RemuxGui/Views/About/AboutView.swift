import SwiftUI
import AppKit

struct AboutView: View {
    @State private var versionInfo = RemuxFFI.version()
    @State private var libraryItems: [LibraryItem] = []

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // App identity
                    HStack(spacing: 12) {
                        AppIconImage()
                            .frame(width: 48, height: 48)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("UBV Remux")
                                .font(.title.bold())
                            Text("Version \(versionInfo.version)")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                        }
                    }

                    Text("Convert Ubiquiti .ubv video files to standard MP4 format.")
                        .font(.body)
                        .foregroundStyle(.secondary)

                    // Metadata
                    GroupBox {
                        Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
                            if !versionInfo.gitCommit.isEmpty {
                                GridRow {
                                    Text("Commit")
                                        .foregroundStyle(.secondary)
                                    Text(String(versionInfo.gitCommit.prefix(10)))
                                        .font(.system(.body, design: .monospaced))
                                        .textSelection(.enabled)
                                }
                            }

                            GridRow {
                                Text("License")
                                    .foregroundStyle(.secondary)
                                HStack(spacing: 6) {
                                    Text("AGPL-3.0")
                                    Link(destination: URL(string: "https://www.gnu.org/licenses/agpl-3.0.html")!) {
                                        Image(systemName: "arrow.up.forward.square")
                                            .font(.caption)
                                    }
                                    .accessibilityLabel("AGPL-3.0 license")
                                }
                            }

                            GridRow {
                                Text("Source")
                                    .foregroundStyle(.secondary)
                                Link("GitHub", destination: URL(string: "https://github.com/petergeneric/unifi-protect-remux")!)
                            }
                        }
                        .font(.body)
                    }

                    Text("\u{00A9} Peter Wright 2020\u{2013}2026")
                        .font(.footnote)
                        .foregroundStyle(.tertiary)

                    Divider()

                    // Credits
                    Text("Third-Party Libraries")
                        .font(.headline)

                    VStack(alignment: .leading, spacing: 3) {
                        ForEach(libraryItems) { item in
                            if let urlString = item.url, let url = URL(string: urlString) {
                                HStack(spacing: 4) {
                                    Text(item.displayText)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                    Link(destination: url) {
                                        Image(systemName: "arrow.up.forward.square")
                                            .font(.system(size: 9))
                                    }
                                    .accessibilityLabel("\(item.sortKey) repository")
                                }
                            } else {
                                Text(item.displayText)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                .padding(24)
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            // Trademark notice
            VStack(spacing: 0) {
                Divider()
                Text("UniFi and UniFi Protect are registered trademarks of Ubiquiti Networks Inc. This software is open source and is unaffiliated with Ubiquiti.")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 24)
                    .padding(.vertical, 10)
            }
        }
        .task {
            libraryItems = buildLibraryItems()
        }
    }

    private func buildLibraryItems() -> [LibraryItem] {
        var items: [LibraryItem] = [
            LibraryItem(text: "CCTV Camera icon by Vectors Market (CC BY 3.0)", url: "https://thenounproject.com/icon/cctv-1925352/"),
            LibraryItem(text: "FFmpeg multimedia framework (LGPL/GPL)", url: "https://ffmpeg.org/"),
        ]

        let licenses = RemuxFFI.licenses()
        for entry in licenses {
            let license = entry.license.isEmpty ? "unknown" : entry.license
            let text = "\(entry.name) \(entry.version) (\(license))"
            let url = entry.repository.isEmpty ? nil : entry.repository
            items.append(LibraryItem(text: text, url: url))
        }

        items.sort { $0.sortKey.localizedCaseInsensitiveCompare($1.sortKey) == .orderedAscending }
        return items
    }
}

private struct LibraryItem: Identifiable {
    let id = UUID()
    let displayText: String
    let url: String?
    let sortKey: String

    init(text: String, url: String? = nil) {
        self.displayText = "\u{2022} \(text)"
        self.sortKey = text
        self.url = url
    }
}

/// Loads the app icon from the bundle's .icns file.
struct AppIconImage: View {
    var body: some View {
        if let icon = loadIcon() {
            Image(nsImage: icon)
                .resizable()
                .aspectRatio(contentMode: .fit)
        } else {
            Image(systemName: "film.stack")
                .font(.system(size: 48))
                .foregroundStyle(Color.accentColor)
        }
    }

    private func loadIcon() -> NSImage? {
        if let icon = NSImage(named: "AppIcon"), icon.isValid { return icon }
        guard let name = Bundle.main.infoDictionary?["CFBundleIconFile"] as? String else { return nil }
        return Bundle.main.image(forResource: name)
    }
}
