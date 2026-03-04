import AppKit
import SwiftUI

@MainActor
enum UbvInfoWindowController {

    private static var openWindows: [NSWindow] = []

    static func open(ubvPath: String, fileName: String, json: String) {
        // Clean up previously closed windows
        openWindows.removeAll { !$0.isVisible }

        let view = UbvInfoView(ubvPath: ubvPath, fileName: fileName, json: json)
        let hostingView = NSHostingView(rootView: view)

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 700),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "UBV Info \u{2014} \(fileName)"
        window.contentView = hostingView
        window.contentMinSize = NSSize(width: 800, height: 500)
        window.isReleasedWhenClosed = false
        window.center()
        window.makeKeyAndOrderFront(nil)

        openWindows.append(window)
    }
}
