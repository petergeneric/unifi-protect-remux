import SwiftUI
import AppKit

struct AboutView: View {
    @State private var versionInfo = RemuxFFI.version()
    @State private var libraryItems: [LibraryItem] = []

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    // App name + version row
                    HStack(spacing: 10) {
                        AppIconImage()
                            .frame(width: 40, height: 40)
                        Text("UBV Remux")
                            .font(.system(size: 22, weight: .bold))
                        Text(versionInfo.version)
                            .font(.system(size: 14))
                            .foregroundStyle(.secondary)
                    }

                    Text("Convert Ubiquiti .ubv video files to standard MP4")
                        .font(.system(size: 13))
                        .foregroundStyle(.secondary)

                    // Details grid
                    Grid(alignment: .leading, verticalSpacing: 4) {
                        if !versionInfo.gitCommit.isEmpty {
                            GridRow {
                                Text("Commit")
                                    .font(.system(size: 13))
                                    .foregroundStyle(.secondary)
                                Text(String(versionInfo.gitCommit.prefix(10)))
                                    .font(.system(size: 13))
                            }
                        }

                        GridRow {
                            Text("License")
                                .font(.system(size: 13))
                                .foregroundStyle(.secondary)
                            HStack(spacing: 6) {
                                Text("Affero GNU Public License 3.0")
                                    .font(.system(size: 13))
                                Button {
                                    openURL("https://www.gnu.org/licenses/agpl-3.0.html")
                                } label: {
                                    Image(systemName: "arrow.up.right.square")
                                        .font(.system(size: 11))
                                        .foregroundStyle(Color.accentColor)
                                }
                                .buttonStyle(.plain)
                                .onHover { inside in
                                    if inside { NSCursor.pointingHand.push() } else { NSCursor.pop() }
                                }
                            }
                        }
                    }

                    // Footer
                    VStack(alignment: .leading, spacing: 4) {
                        Text("\u{00A9} Peter Wright 2020-2026")
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                        Text("https://github.com/petergeneric/unifi-protect-remux")
                            .font(.system(size: 12))
                            .foregroundStyle(Color.accentColor)
                            .underline()
                            .onTapGesture {
                                openURL("https://github.com/petergeneric/unifi-protect-remux")
                            }
                            .onHover { inside in
                                if inside {
                                    NSCursor.pointingHand.push()
                                } else {
                                    NSCursor.pop()
                                }
                            }
                    }

                    Divider()
                        .padding(.vertical, 4)

                    // Credits & Third-Party Libraries
                    Text("Credits & Third-Party Libraries")
                        .font(.system(size: 15, weight: .semibold))

                    ForEach(libraryItems) { item in
                        HStack(spacing: 4) {
                            Text(item.displayText)
                                .font(.system(size: 12))
                                .foregroundStyle(.secondary)
                            if let url = item.url {
                                Button {
                                    openURL(url)
                                } label: {
                                    Image(systemName: "arrow.up.right.square")
                                        .font(.system(size: 10))
                                        .foregroundStyle(Color.accentColor)
                                }
                                .buttonStyle(.plain)
                                .onHover { inside in
                                    if inside { NSCursor.pointingHand.push() } else { NSCursor.pop() }
                                }
                            }
                        }
                    }

                    Spacer().frame(height: 140)
                }
                .padding(EdgeInsets(top: 24, leading: 28, bottom: 0, trailing: 28))
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            // Trademark notice — pinned to bottom
            VStack(spacing: 0) {
                Divider()
                Text("UniFi and UniFi Protect are registered trademarks of Ubiquiti Networks Inc. This software is open source and is unaffiliated with Ubiquiti.")
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .padding(EdgeInsets(top: 8, leading: 28, bottom: 16, trailing: 28))
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

    private func openURL(_ urlString: String) {
        guard let url = URL(string: urlString) else { return }
        NSWorkspace.shared.open(url)
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
