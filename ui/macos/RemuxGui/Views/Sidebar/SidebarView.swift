import SwiftUI

struct SidebarView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        VStack(spacing: 2) {
            Spacer().frame(height: 8)

            NavButton(
                label: "Files",
                icon: "folder",
                tab: .files,
                badge: vm.files.isEmpty ? nil : "\(vm.files.count)"
            )

            NavButton(
                label: "Settings",
                icon: "gearshape",
                tab: .settings
            )

            NavButton(
                label: "Log",
                icon: "doc.text",
                tab: .log,
                badge: vm.errorCount > 0 ? "\(vm.errorCount)" : nil,
                badgeColor: .statusFailed
            )

            NavButton(
                label: "Cameras",
                icon: "video",
                tab: .cameras
            )

            Spacer()

            NavButton(
                label: "About",
                icon: "info.circle",
                tab: .about
            )

            Spacer().frame(height: 8)
        }
        .padding(.horizontal, 8)
        .frame(maxHeight: .infinity)
        .background(.ultraThinMaterial)
    }
}

private struct NavButton: View {
    @Environment(AppViewModel.self) private var vm
    let label: String
    let icon: String
    let tab: NavigationTab
    var badge: String? = nil
    var badgeColor: Color = .accentColor
    @State private var isHovered = false

    private var isActive: Bool { vm.currentView == tab }

    private var accessibilityLabel: String {
        if let badge {
            "\(label), \(badge)"
        } else {
            label
        }
    }

    var body: some View {
        Button {
            withAnimation(.easeInOut(duration: 0.15)) {
                vm.currentView = tab
            }
        } label: {
            HStack(spacing: 8) {
                Image(systemName: isActive ? activeIcon : icon)
                    .font(.system(size: 13))
                    .frame(width: 20)
                    .foregroundStyle(isActive ? Color.accentColor : .secondary)
                Text(label)
                    .font(.system(size: 13))
                Spacer()
                if let badge {
                    Text(badge)
                        .font(.caption2.weight(.medium).monospacedDigit())
                        .padding(.horizontal, 6)
                        .padding(.vertical, 1)
                        .background(badgeColor.opacity(isActive ? 1 : 0.6))
                        .foregroundStyle(.white)
                        .clipShape(Capsule())
                        .accessibilityHidden(true)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .foregroundStyle(isActive ? .primary : .secondary)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(isActive ? Color.accentColor.opacity(0.12) : (isHovered ? Color.primary.opacity(0.04) : .clear))
            )
        }
        .buttonStyle(.plain)
        .focusEffectDisabled()
        .onHover { isHovered = $0 }
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(isActive ? .isSelected : [])
    }

    /// Return the filled variant of the SF Symbol when active.
    private var activeIcon: String {
        switch icon {
        case "folder": "folder.fill"
        case "gearshape": "gearshape.fill"
        case "doc.text": "doc.text.fill"
        case "video": "video.fill"
        case "info.circle": "info.circle.fill"
        default: icon
        }
    }
}
