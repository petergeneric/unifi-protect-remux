import SwiftUI
import AppKit

@main
struct RemuxGuiApp: App {
    @NSApplicationDelegateAdaptor private var appDelegate: AppDelegate
    @State private var viewModel = AppViewModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(viewModel)
                .onAppear {
                    handleCommandLineArgs()
                }
                .onOpenURL { url in
                    _ = viewModel.addFiles([url])
                }
        }
        .defaultSize(width: 1100, height: 700)
        .windowResizability(.contentSize)
        .commands {
            CommandGroup(replacing: .appInfo) {
                Button("About UBV Remux") {
                    showNativeAbout()
                }
            }
            CommandMenu("Navigate") {
                Button("Files") { viewModel.currentView = .files }
                    .keyboardShortcut("1", modifiers: .command)
                Button("Settings") { viewModel.currentView = .settings }
                    .keyboardShortcut("2", modifiers: .command)
                Button("Log") { viewModel.currentView = .log }
                    .keyboardShortcut("3", modifiers: .command)
                Button("Cameras") { viewModel.currentView = .cameras }
                    .keyboardShortcut("4", modifiers: .command)
                Button("About") { viewModel.currentView = .about }
                    .keyboardShortcut("5", modifiers: .command)
            }
        }
    }

    private func showNativeAbout() {
        let info = RemuxFFI.version()
        var options: [NSApplication.AboutPanelOptionKey: Any] = [
            .credits: NSAttributedString(
                string: "Converts Ubiquiti .ubv video files to standard MP4 format.",
                attributes: [
                    .font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize),
                    .foregroundColor: NSColor.secondaryLabelColor,
                ]
            ),
            .version: info.gitCommit.isEmpty
                ? ""
                : String(info.gitCommit.prefix(8)),
        ]
        if let icon = NSImage(named: "AppIcon") ?? appIconFromBundle() {
            options[.applicationIcon] = icon
        }
        NSApplication.shared.orderFrontStandardAboutPanel(options: options)
    }

    private func appIconFromBundle() -> NSImage? {
        guard let iconName = Bundle.main.infoDictionary?["CFBundleIconFile"] as? String else {
            return nil
        }
        guard let url = Bundle.main.url(forResource: iconName, withExtension: "icns")
                ?? Bundle.main.url(forResource: iconName, withExtension: nil) else {
            return nil
        }
        return NSImage(contentsOf: url)
    }

    private func handleCommandLineArgs() {
        let args = CommandLine.arguments.dropFirst()
        let urls = args.compactMap { arg -> URL? in
            let lower = arg.lowercased()
            guard lower.hasSuffix(".ubv") || lower.hasSuffix(".ubv.gz") else { return nil }
            let url = URL(fileURLWithPath: arg)
            // Under sandbox, CLI paths have no security scope — verify readable
            guard FileManager.default.isReadableFile(atPath: url.path) else { return nil }
            return url
        }
        if !urls.isEmpty {
            _ = viewModel.addFiles(urls)
        }
    }
}

class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}
