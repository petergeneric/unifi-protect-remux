import SwiftUI
import UniformTypeIdentifiers

struct FilesView: View {
    @Environment(AppViewModel.self) private var vm
    @State private var lowResWarningURLs: [URL] = []
    @State private var showLowResAlert = false
    @State private var isDropTargeted = false

    var body: some View {
        VStack(spacing: 0) {
            // Main content
            HStack(spacing: 0) {
                // File list (left pane)
                fileListPane
                    .frame(minWidth: 300)

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
        .onDrop(of: [.fileURL], isTargeted: $isDropTargeted) { providers in
            guard !providers.isEmpty, !vm.isBusy else { return false }
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
                List(vm.files, id: \.id, selection: Binding(
                    get: { vm.selectedFile?.id },
                    set: { id in vm.selectedFile = vm.files.first { $0.id == id } }
                )) { file in
                    FileRowView(file: file)
                }
                .listStyle(.inset(alternatesRowBackgrounds: true))
            }
        }
    }

    private var dropZone: some View {
        VStack(spacing: 10) {
            Image(systemName: "arrow.down.doc")
                .font(.system(size: 36, weight: .light))
                .foregroundStyle(.tertiary)
            Text("Drop .ubv files here")
                .font(.headline)
                .foregroundStyle(.secondary)
            Text("or click Browse below")
                .font(.subheadline)
                .foregroundStyle(.tertiary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .strokeBorder(
                    isDropTargeted ? Color.accentColor : Color.secondary.opacity(0.2),
                    style: StrokeStyle(lineWidth: isDropTargeted ? 2 : 1.5, dash: [8, 4])
                )
                .padding(16)
        )
        .background(isDropTargeted ? Color.accentColor.opacity(0.04) : .clear)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Drop zone. Drop .ubv files here, or click Browse below.")
    }

    private var bottomBar: some View {
        HStack(spacing: 12) {
            Button("Convert All") {
                vm.startAll()
            }
            .disabled(vm.isBusy || vm.files.isEmpty)

            if vm.isProcessing {
                Button("Cancel", role: .cancel) {
                    vm.cancel()
                }
            }

            Spacer()

            if vm.hasProgressInfo {
                ProgressView(value: vm.progressPercent, total: 100)
                    .progressViewStyle(.linear)
                    .frame(width: 140)
                    .accessibilityLabel(vm.progressText)
                    .accessibilityValue("\(Int(vm.progressPercent)) percent")
                Text(vm.progressText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
                    .accessibilityHidden(true)
            }

            Button("Browse\u{2026}") {
                browseFiles()
            }
            .disabled(vm.isBusy)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
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
