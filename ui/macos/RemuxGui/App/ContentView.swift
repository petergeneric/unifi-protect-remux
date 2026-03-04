import SwiftUI

struct ContentView: View {
    @Environment(AppViewModel.self) private var vm

    private var windowTitle: String {
        switch vm.currentView {
        case 0: "UBV Remux"
        case 1: "Settings"
        case 2: "Log"
        case 3: "Cameras"
        case 4: "About"
        default: "UBV Remux"
        }
    }

    var body: some View {
        HStack(spacing: 0) {
            SidebarView()
                .frame(width: 200)

            Divider()

            ZStack {
                FilesView()
                    .opacity(vm.currentView == 0 ? 1 : 0)
                    .allowsHitTesting(vm.currentView == 0)

                SettingsView()
                    .opacity(vm.currentView == 1 ? 1 : 0)
                    .allowsHitTesting(vm.currentView == 1)

                LogView()
                    .opacity(vm.currentView == 2 ? 1 : 0)
                    .allowsHitTesting(vm.currentView == 2)

                CamerasView()
                    .opacity(vm.currentView == 3 ? 1 : 0)
                    .allowsHitTesting(vm.currentView == 3)

                AboutView()
                    .opacity(vm.currentView == 4 ? 1 : 0)
                    .allowsHitTesting(vm.currentView == 4)
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
