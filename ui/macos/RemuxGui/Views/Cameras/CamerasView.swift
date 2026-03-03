import SwiftUI

struct CamerasView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text("Cameras")
                    .font(.headline)
                Spacer()
                Button(vm.cameraSaveLabel) {
                    vm.saveCamerasExplicit()
                }
                .disabled(!vm.hasUnsavedCameraChanges && vm.cameraSaveLabel == "Save")
            }
            .padding(.horizontal)
            .padding(.vertical, 8)

            Text("Assign friendly names to cameras by MAC address. Names are used as output file prefixes.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.horizontal)
                .padding(.bottom, 8)

            Divider()

            if vm.cameras.isEmpty {
                Spacer()
                VStack(spacing: 8) {
                    Image(systemName: "video.slash")
                        .font(.system(size: 32))
                        .foregroundStyle(.secondary)
                    Text("No cameras registered")
                        .foregroundStyle(.secondary)
                    Text("Cameras appear automatically when you add .ubv files")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            } else {
                // Column headers
                HStack {
                    Text("MAC Address")
                        .font(.caption.bold())
                        .frame(width: 160, alignment: .leading)
                    Text("Friendly Name")
                        .font(.caption.bold())
                    Spacer()
                }
                .padding(.horizontal)
                .padding(.vertical, 4)
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
        HStack {
            Text(camera.macAddressFormatted)
                .font(.system(.body, design: .monospaced))
                .frame(width: 160, alignment: .leading)

            TextField("Camera name", text: $camera.friendlyName)
                .textFieldStyle(.roundedBorder)
                .onChange(of: camera.friendlyName) {
                    vm.hasUnsavedCameraChanges = true
                    vm.refreshAllCameraNames()
                }

            if isHovered {
                Button {
                    vm.removeCamera(camera)
                } label: {
                    Image(systemName: "trash")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
        }
        .onHover { isHovered = $0 }
    }
}
