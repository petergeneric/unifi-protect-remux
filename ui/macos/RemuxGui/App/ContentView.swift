import SwiftUI

struct ContentView: View {
    @Environment(AppViewModel.self) private var vm

    private var windowTitle: String {
        switch vm.currentView {
        case .files: "UBV Remux"
        case .settings: "Settings"
        case .log: "Log"
        case .cameras: "Cameras"
        case .about: "About"
        }
    }

    var body: some View {
        HStack(spacing: 0) {
            SidebarView()
                .frame(width: 200)

            Divider()

            ZStack {
                FilesView()
                    .opacity(vm.currentView == .files ? 1 : 0)
                    .allowsHitTesting(vm.currentView == .files)
                    .accessibilityHidden(vm.currentView != .files)

                SettingsView()
                    .opacity(vm.currentView == .settings ? 1 : 0)
                    .allowsHitTesting(vm.currentView == .settings)
                    .accessibilityHidden(vm.currentView != .settings)

                LogView()
                    .opacity(vm.currentView == .log ? 1 : 0)
                    .allowsHitTesting(vm.currentView == .log)
                    .accessibilityHidden(vm.currentView != .log)

                CamerasView()
                    .opacity(vm.currentView == .cameras ? 1 : 0)
                    .allowsHitTesting(vm.currentView == .cameras)
                    .accessibilityHidden(vm.currentView != .cameras)

                AboutView()
                    .opacity(vm.currentView == .about ? 1 : 0)
                    .allowsHitTesting(vm.currentView == .about)
                    .accessibilityHidden(vm.currentView != .about)
            }
        }
        .frame(minWidth: 760, minHeight: 480)
        .navigationTitle(windowTitle)
        .background(Color(nsColor: .windowBackgroundColor))
        .onChange(of: vm.showUbvInfo) { _, show in
            if show {
                UbvInfoWindowController.open(
                    ubvPath: vm.ubvInfoPath,
                    fileName: vm.ubvInfoFileName,
                    json: vm.ubvInfoJSON
                )
                vm.showUbvInfo = false
            }
        }
    }
}
