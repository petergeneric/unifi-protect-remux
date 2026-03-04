import SwiftUI

struct CamerasView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text("Cameras")
                    .font(.title2.bold())
                Spacer()
                Button(vm.cameraSaveLabel) {
                    vm.saveCamerasExplicit()
                }
                .disabled(!vm.hasUnsavedCameraChanges && vm.cameraSaveLabel == "Save")
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            Text("Assign friendly names to cameras by MAC address. Names are used as output file prefixes.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 16)
                .padding(.bottom, 8)

            Divider()

            if vm.cameras.isEmpty {
                Spacer()
                VStack(spacing: 8) {
                    Image(systemName: "video.slash")
                        .font(.system(size: 32, weight: .light))
                        .foregroundStyle(.tertiary)
                    Text("No cameras registered")
                        .font(.headline)
                        .foregroundStyle(.secondary)
                    Text("Cameras appear automatically when you add .ubv files")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
                Spacer()
            } else {
                // Column headers
                HStack {
                    Text("MAC Address")
                        .frame(width: 160, alignment: .leading)
                    Text("Friendly Name")
                    Spacer()
                }
                .font(.caption.bold())
                .foregroundStyle(.secondary)
                .padding(.horizontal, 16)
                .padding(.vertical, 5)
                .background(Color(nsColor: .controlBackgroundColor))

                // Camera list
                List {
                    ForEach(vm.cameras) { camera in
                        CameraRowView(camera: camera)
                    }
                }
                .listStyle(.plain)
            }
        }
    }
}

private struct CameraRowView: View {
    @Environment(AppViewModel.self) private var vm
    @Bindable var camera: CameraEntry
    @State private var isHovered = false

    var body: some View {
        HStack(spacing: 8) {
            Text(camera.macAddressFormatted)
                .font(.system(.body, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 160, alignment: .leading)
                .textSelection(.enabled)

            TextField("Camera name", text: $camera.friendlyName)
                .textFieldStyle(.roundedBorder)
                .onChange(of: camera.friendlyName) {
                    vm.hasUnsavedCameraChanges = true
                    vm.refreshAllCameraNames()
                }

            Button {
                vm.removeCamera(camera)
            } label: {
                Image(systemName: "trash")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .opacity(isHovered ? 1 : 0)
            .help("Remove camera")
            .accessibilityLabel("Remove camera \(camera.displayName)")
        }
        .onHover { isHovered = $0 }
    }
}
