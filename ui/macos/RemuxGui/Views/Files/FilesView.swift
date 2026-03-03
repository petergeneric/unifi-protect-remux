import SwiftUI
import UniformTypeIdentifiers

struct FilesView: View {
    @Environment(AppViewModel.self) private var vm
    @State private var lowResWarningURLs: [URL] = []
    @State private var showLowResAlert = false

    var body: some View {
        VStack(spacing: 0) {
            // Main content
            HStack(spacing: 0) {
                // File list (left pane)
                fileListPane

                // Detail pane (right)
                if let file = vm.selectedFile {
                    Divider()
                    FileDetailView(file: file)
                        .frame(width: 320)
                }
            }

            Divider()

            // Bottom bar
            bottomBar
        }
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            handleDrop(providers)
            return true
        }
        .alert("Low Resolution Files", isPresented: $showLowResAlert) {
            Button("Add Anyway") {
                vm.addWarnedFiles(lowResWarningURLs)
                lowResWarningURLs = []
            }
            Button("Skip", role: .cancel) {
                lowResWarningURLs = []
            }
        } message: {
            Text("Some files appear to be low-resolution recordings (timelapse or secondary stream). Add them anyway?")
        }
    }

    private var fileListPane: some View {
        Group {
            if vm.files.isEmpty {
                dropZone
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(vm.files, id: \.id) { file in
                            FileRowView(file: file)
                                .padding(.horizontal, 10)
                                .padding(.vertical, 4)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .contentShape(Rectangle())
                                .background(
                                    vm.selectedFile?.id == file.id
                                        ? Color.accentColor.opacity(0.2)
                                        : Color.clear
                                )
                                .onTapGesture {
                                    vm.selectedFile = file
                                }
                        }
                    }
                }
                .onChange(of: vm.files.count) {
                    if vm.selectedFile == nil, let first = vm.files.first {
                        vm.selectedFile = first
                    }
                }
            }
        }
    }

    private var dropZone: some View {
        VStack(spacing: 12) {
            Image(systemName: "arrow.down.doc")
                .font(.system(size: 40))
                .foregroundStyle(Color.secondary)
            Text("Drop .ubv files here")
                .foregroundStyle(Color.secondary)
            Text("or use Browse below")
                .font(.caption)
                .foregroundStyle(Color.secondary.opacity(0.7))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var bottomBar: some View {
        HStack {
            Button("Convert All") {
                vm.startAll()
            }
            .disabled(vm.isBusy || vm.files.isEmpty)

            if vm.isProcessing {
                Button("Cancel") {
                    vm.cancel()
                }
            }

            Spacer()

            if vm.hasProgressInfo {
                ProgressView(value: vm.progressPercent, total: 100)
                    .frame(width: 120)
                Text(vm.progressText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Button("Browse...") {
                browseFiles()
            }
            .disabled(vm.isBusy)
        }
        .padding(10)
        .background(.bar)
    }

    private func browseFiles() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowedContentTypes = [
            UTType(filenameExtension: "ubv") ?? .data,
            UTType(filenameExtension: "gz") ?? .data,
        ]
        if panel.runModal() == .OK {
            let warned = vm.addFiles(panel.urls)
            if !warned.isEmpty {
                lowResWarningURLs = warned
                showLowResAlert = true
            }
        }
    }

    private func handleDrop(_ providers: [NSItemProvider]) {
        let providers = providers
        Task { @MainActor in
            var urls: [URL] = []
            for provider in providers {
                if let url = try? await provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier) as? Data {
                    if let fileURL = URL(dataRepresentation: url, relativeTo: nil) {
                        urls.append(fileURL)
                    }
                }
            }
            let warned = vm.addFiles(urls)
            if !warned.isEmpty {
                lowResWarningURLs = warned
                showLowResAlert = true
            }
        }
    }
}
