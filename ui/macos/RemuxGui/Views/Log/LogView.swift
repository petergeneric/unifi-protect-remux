import SwiftUI

struct LogView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        @Bindable var vm = vm
        VStack(spacing: 0) {
            // Top toolbar
            HStack {
                Text("Log")
                    .font(.title2.bold())
                Spacer()
                Button("Clear", systemImage: "trash") {
                    vm.clearLog()
                }
                .disabled(vm.logLines.isEmpty)
                Button("Export\u{2026}", systemImage: "square.and.arrow.up") {
                    exportLog()
                }
                .disabled(vm.logLines.isEmpty)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            // File filter banner
            if let label = vm.logFileFilterLabel {
                HStack(spacing: 6) {
                    Image(systemName: "line.3.horizontal.decrease.circle.fill")
                        .foregroundStyle(Color.accentColor)
                    Text("Showing: \(label)")
                        .font(.caption)
                    Spacer()
                    Button("Show All") {
                        vm.clearLogFileFilter()
                    }
                    .controlSize(.small)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 6)
                .background(Color.accentColor.opacity(0.06))
            }

            // Filter bar
            HStack(spacing: 8) {
                HStack(spacing: 4) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(.tertiary)
                    TextField("Filter\u{2026}", text: $vm.logSearchText)
                        .textFieldStyle(.plain)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(Color(nsColor: .controlBackgroundColor))
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.secondary.opacity(0.15)))
                .frame(maxWidth: 220)

                filterPill("All", level: nil, count: vm.logLines.count)
                filterPill("Info", level: .info, count: vm.infoCount)
                filterPill("Warn", level: .warn, count: vm.warnCount, color: .logWarn)
                filterPill("Error", level: .error, count: vm.errorCount, color: .logError)

                Spacer()
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 6)

            Divider()

            // Log entries
            if vm.filteredLogLines.isEmpty {
                Spacer()
                VStack(spacing: 6) {
                    Image(systemName: "doc.text")
                        .font(.system(size: 28, weight: .light))
                        .foregroundStyle(.tertiary)
                    Text(vm.logLines.isEmpty ? "No log entries" : "No matching entries")
                        .foregroundStyle(.secondary)
                }
                Spacer()
            } else {
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
            }

            // Status bar
            HStack(spacing: 12) {
                Text("\(vm.logLines.count) entries")
                    .monospacedDigit()
                Spacer()
                if vm.errorCount > 0 {
                    Label("\(vm.errorCount) errors", systemImage: "xmark.circle")
                        .foregroundStyle(Color.logError)
                }
                if vm.warnCount > 0 {
                    Label("\(vm.warnCount) warnings", systemImage: "exclamationmark.triangle")
                        .foregroundStyle(Color.logWarn)
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 16)
            .padding(.vertical, 5)
            .background(.bar)
        }
    }

    private func filterPill(_ label: String, level: LogLevel?, count: Int, color: Color = .accentColor) -> some View {
        let isActive = vm.logFilterLevel == level
        return Button {
            vm.logFilterLevel = level
        } label: {
            Text("\(label) \(count)")
                .font(.caption.monospacedDigit())
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(isActive ? color.opacity(0.15) : .clear)
                .foregroundStyle(isActive ? color : .secondary)
                .clipShape(Capsule())
                .overlay(Capsule().stroke(isActive ? color.opacity(0.3) : Color.secondary.opacity(0.2), lineWidth: 1))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Filter \(label)")
        .accessibilityValue("\(count)")
        .accessibilityAddTraits(isActive ? .isSelected : [])
    }

    private func exportLog() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.plainText]
        panel.nameFieldStringValue = "remux-log.txt"
        if panel.runModal() == .OK, let url = panel.url {
            let text = vm.logLines.map { entry in
                "\(entry.timestampLabel) [\(entry.level.rawValue)] \(entry.message)"
            }.joined(separator: "\n")
            do {
                try text.write(to: url, atomically: true, encoding: .utf8)
            } catch {
                vm.logLines.append(LogEntry(level: .error, message: "Failed to export log: \(error.localizedDescription)"))
            }
        }
    }
}
