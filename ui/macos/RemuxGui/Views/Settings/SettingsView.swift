import SwiftUI

struct SettingsView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        @Bindable var vm = vm
        ScrollView {
            LazyVGrid(columns: [
                GridItem(.flexible(), spacing: 16),
                GridItem(.flexible(), spacing: 16),
            ], spacing: 16) {
                // Streams card
                SettingsCard(title: "Streams", icon: "waveform") {
                    Toggle("Include Video", isOn: $vm.withVideo)
                    Toggle("Include Audio", isOn: $vm.withAudio)
                }

                // Output Format card
                SettingsCard(title: "Output Format", icon: "film") {
                    Toggle("MP4 Output", isOn: $vm.mp4Output)
                    Toggle("Fast Start (moov atom)", isOn: $vm.fastStart)
                }

                // Output Location card
                SettingsCard(title: "Output Location", icon: "folder") {
                    HStack {
                        Text(vm.outputFolder == RemuxConfig.defaultOutputFolder
                             ? "Same as source"
                             : vm.outputFolder)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer()
                        Button("Browse...") {
                            chooseOutputFolder()
                        }
                    }
                    Button("Reset to Source Folder") {
                        vm.outputFolder = RemuxConfig.defaultOutputFolder
                    }
                    .font(.caption)
                }

                // Advanced card
                SettingsCard(title: "Advanced", icon: "slider.horizontal.3") {
                    HStack {
                        Text("Force Framerate:")
                        TextField("0 = auto", value: $vm.forceRate, format: .number)
                            .frame(width: 80)
                            .textFieldStyle(.roundedBorder)
                    }
                    HStack {
                        Text("Video Track:")
                        TextField("0 = default", value: $vm.videoTrack, format: .number)
                            .frame(width: 80)
                            .textFieldStyle(.roundedBorder)
                    }
                }
            }
            .padding()
        }
    }

    private func chooseOutputFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            vm.outputFolder = url.path
        }
    }
}

private struct SettingsCard<Content: View>: View {
    let title: String
    let icon: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 6) {
                Image(systemName: icon)
                    .foregroundStyle(Color.accentColor)
                Text(title)
                    .font(.headline)
            }

            content
        }
        .padding()
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }
}
