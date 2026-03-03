import SwiftUI

struct SidebarView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        VStack(spacing: 4) {
            Spacer().frame(height: 12)

            NavButton(
                label: "Files",
                icon: "folder",
                index: 0,
                badge: vm.files.isEmpty ? nil : "\(vm.files.count)"
            )

            NavButton(
                label: "Settings",
                icon: "gearshape",
                index: 1
            )

            NavButton(
                label: "Log",
                icon: "doc.text",
                index: 2,
                badge: vm.errorCount > 0 ? "\(vm.errorCount)" : nil,
                badgeColor: .red
            )

            NavButton(
                label: "Cameras",
                icon: "video",
                index: 3
            )

            Spacer()

            NavButton(
                label: "About",
                icon: "info.circle",
                index: 4
            )

            Spacer().frame(height: 12)
        }
        .padding(.horizontal, 8)
        .frame(maxHeight: .infinity)
    }
}

private struct NavButton: View {
    @Environment(AppViewModel.self) private var vm
    let label: String
    let icon: String
    let index: Int
    var badge: String? = nil
    var badgeColor: Color = .accentColor

    private var isActive: Bool { vm.currentView == index }

    var body: some View {
        Button {
            vm.currentView = index
        } label: {
            HStack(spacing: 8) {
                Image(systemName: icon)
                    .frame(width: 20)
                Text(label)
                Spacer()
                if let badge {
                    Text(badge)
                        .font(.caption2.weight(.semibold))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(isActive ? badgeColor : badgeColor.opacity(0.3))
                        .foregroundStyle(.white)
                        .clipShape(Capsule())
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .foregroundStyle(isActive ? .primary : .secondary)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(isActive ? Color.accentColor.opacity(0.15) : .clear)
            )
        }
        .buttonStyle(.plain)
        .focusEffectDisabled()
    }
}
