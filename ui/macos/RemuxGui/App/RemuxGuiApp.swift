import SwiftUI

@main
struct RemuxGuiApp: App {
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
        .commands {
            CommandMenu("Navigate") {
                Button("Files") { viewModel.currentView = 0 }
                    .keyboardShortcut("1", modifiers: .command)
                Button("Settings") { viewModel.currentView = 1 }
                    .keyboardShortcut("2", modifiers: .command)
                Button("Log") { viewModel.currentView = 2 }
                    .keyboardShortcut("3", modifiers: .command)
                Button("Cameras") { viewModel.currentView = 3 }
                    .keyboardShortcut("4", modifiers: .command)
                Button("About") { viewModel.currentView = 4 }
                    .keyboardShortcut("5", modifiers: .command)
            }
        }
    }

    private func handleCommandLineArgs() {
        let args = CommandLine.arguments.dropFirst()
        let urls = args.compactMap { arg -> URL? in
            let lower = arg.lowercased()
            guard lower.hasSuffix(".ubv") || lower.hasSuffix(".ubv.gz") else { return nil }
            return URL(fileURLWithPath: arg)
        }
        if !urls.isEmpty {
            _ = viewModel.addFiles(urls)
        }
    }
}
