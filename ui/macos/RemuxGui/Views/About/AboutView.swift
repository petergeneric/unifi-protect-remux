import SwiftUI

struct AboutView: View {
    @State private var versionInfo = RemuxFFI.version()
    @State private var licenses: [LicenseEntry] = []

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                Spacer().frame(height: 20)

                // App icon placeholder + title
                Image(systemName: "film.stack")
                    .font(.system(size: 48))
                    .foregroundStyle(Color.accentColor)

                Text("UBV Remux")
                    .font(.title.bold())

                Text("v\(versionInfo.version)")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                Text("Converts Ubiquiti .ubv video files to standard MP4 format.")
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.secondary)

                if !versionInfo.gitCommit.isEmpty {
                    Text("Commit: \(String(versionInfo.gitCommit.prefix(8)))")
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(.secondary)
                }

                Divider()
                    .padding(.horizontal, 40)

                // Credits
                VStack(alignment: .leading, spacing: 8) {
                    Text("Credits")
                        .font(.headline)

                    creditRow("FFmpeg", detail: "Licensed under LGPL 2.1+")
                    creditRow("Rust", detail: "Systems programming language")

                    if !licenses.isEmpty {
                        Text("Third-Party Libraries")
                            .font(.subheadline.bold())
                            .padding(.top, 8)

                        ForEach(licenses) { entry in
                            creditRow(
                                "\(entry.name) \(entry.version)",
                                detail: entry.license
                            )
                        }
                    }
                }
                .frame(maxWidth: 500, alignment: .leading)

                Spacer().frame(height: 20)

                Text("Ubiquiti, UniFi, and UniFi Protect are trademarks of Ubiquiti Inc. This project is not affiliated with or endorsed by Ubiquiti Inc.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 40)

                Spacer().frame(height: 20)
            }
            .frame(maxWidth: .infinity)
        }
        .task {
            licenses = RemuxFFI.licenses()
        }
    }

    private func creditRow(_ name: String, detail: String) -> some View {
        HStack(alignment: .top) {
            Text(name)
                .font(.caption.bold())
                .frame(width: 180, alignment: .leading)
            Text(detail)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }
}
