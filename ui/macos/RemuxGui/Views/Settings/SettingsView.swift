import SwiftUI

struct SettingsView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        @Bindable var vm = vm
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Settings")
                    .font(.title2.bold())

                LazyVGrid(columns: [
                    GridItem(.flexible(), spacing: 16),
                    GridItem(.flexible(), spacing: 16),
                ], spacing: 16) {
                    // Streams
                    GroupBox {
                        VStack(alignment: .leading, spacing: 8) {
                            Toggle("Include Video", isOn: $vm.withVideo)
                            Toggle("Include Audio", isOn: $vm.withAudio)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    } label: {
                        Label("Streams", systemImage: "waveform")
                    }

                    // Output Format
                    GroupBox {
                        VStack(alignment: .leading, spacing: 8) {
                            Toggle("MP4 Output", isOn: $vm.mp4Output)
                            Toggle("Fast Start (moov atom)", isOn: $vm.fastStart)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    } label: {
                        Label("Output Format", systemImage: "film")
                    }

                    // Output Location
                    GroupBox {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Image(systemName: "folder")
                                    .foregroundStyle(.secondary)
                                    .accessibilityHidden(true)
                                Text(vm.outputFolder == RemuxConfig.defaultOutputFolder
                                     ? "Same as source"
                                     : vm.outputFolder)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                                    .foregroundStyle(.secondary)
                                    .help(vm.outputFolder)
                                Spacer()
                                Button("Browse\u{2026}") {
                                    chooseOutputFolder()
                                }
                                .controlSize(.small)
                            }
                            if vm.outputFolder != RemuxConfig.defaultOutputFolder {
                                Button("Reset to Source Folder") {
                                    vm.outputFolder = RemuxConfig.defaultOutputFolder
                                }
                                .controlSize(.small)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    } label: {
                        Label("Output Location", systemImage: "folder")
                    }

                    // Advanced
                    GroupBox {
                        VStack(alignment: .leading, spacing: 8) {
                            LabeledContent("Force Framerate") {
                                TextField("auto", value: $vm.forceRate, format: .number)
                                    .frame(width: 70)
                                    .textFieldStyle(.roundedBorder)
                                    .monospacedDigit()
                            }
                            LabeledContent("Video Track") {
                                TextField("default", value: $vm.videoTrack, format: .number)
                                    .frame(width: 70)
                                    .textFieldStyle(.roundedBorder)
                                    .monospacedDigit()
                            }

                            Text("Set to 0 for automatic detection.")
                                .font(.caption)
                                .foregroundStyle(.tertiary)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    } label: {
                        Label("Advanced", systemImage: "slider.horizontal.3")
                    }
                }
            }
            .padding(20)
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
