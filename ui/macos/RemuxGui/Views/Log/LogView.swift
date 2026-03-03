import SwiftUI

struct LogView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        @Bindable var vm = vm
        VStack(spacing: 0) {
            // Top toolbar
            HStack {
                Text("Log")
                    .font(.headline)
                Spacer()
                Button("Clear") {
                    vm.clearLog()
                }
                Button("Export...") {
                    exportLog()
                }
            }
            .padding(.horizontal)
            .padding(.vertical, 8)

            // File filter banner
            if let label = vm.logFileFilterLabel {
                HStack {
                    Text("Filtered to: \(label)")
                        .font(.caption)
                    Spacer()
                    Button("Show all") {
                        vm.clearLogFileFilter()
                    }
                    .font(.caption)
                }
                .padding(.horizontal)
                .padding(.vertical, 4)
                .background(Color.accentColor.opacity(0.1))
            }

            // Filter bar
            HStack(spacing: 8) {
                TextField("Search...", text: $vm.logSearchText)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 200)

                filterPill("All", count: vm.logLines.count)
                filterPill("Info", count: vm.infoCount)
                filterPill("Warn", count: vm.warnCount)
                filterPill("Error", count: vm.errorCount)

                Spacer()
            }
            .padding(.horizontal)
            .padding(.vertical, 6)

            Divider()

            // Log entries
            ScrollViewReader { proxy in
                List(vm.filteredLogLines) { entry in
                    LogRowView(entry: entry)
                }
                .listStyle(.plain)
                .onChange(of: vm.filteredLogLines.count) {
                    if let last = vm.filteredLogLines.last {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }

            // Status bar
            HStack {
                Text("\(vm.logLines.count) entries")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                if vm.errorCount > 0 {
                    Text("\(vm.errorCount) errors")
                        .font(.caption)
                        .foregroundStyle(Color.logError)
                }
                if vm.warnCount > 0 {
                    Text("\(vm.warnCount) warnings")
                        .font(.caption)
                        .foregroundStyle(Color.logWarn)
                }
            }
            .padding(.horizontal)
            .padding(.vertical, 4)
            .background(.bar)
        }
    }

    private func filterPill(_ level: String, count: Int) -> some View {
        let isActive = vm.logFilterLevel == level
        return Button {
            vm.logFilterLevel = level
        } label: {
            Text("\(level) (\(count))")
                .font(.caption)
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(isActive ? Color.accentColor : Color.clear)
                .foregroundStyle(isActive ? .white : .secondary)
                .clipShape(Capsule())
                .overlay(Capsule().stroke(Color.secondary.opacity(0.3), lineWidth: isActive ? 0 : 1))
        }
        .buttonStyle(.plain)
    }

    private func exportLog() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.plainText]
        panel.nameFieldStringValue = "remux-log.txt"
        if panel.runModal() == .OK, let url = panel.url {
            let text = vm.logLines.map { entry in
                "\(entry.timestampLabel) [\(entry.level)] \(entry.message)"
            }.joined(separator: "\n")
            try? text.write(to: url, atomically: true, encoding: .utf8)
        }
    }
}
