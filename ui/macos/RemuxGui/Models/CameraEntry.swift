import Foundation

@Observable
final class CameraEntry: Identifiable {
    let id = UUID()
    var macAddress: String
    var friendlyName: String

    var macAddressFormatted: String {
        RemuxFFI.formatMAC(macAddress) ?? macAddress
    }

    var displayName: String {
        friendlyName.trimmingCharacters(in: .whitespaces).isEmpty
            ? macAddressFormatted
            : friendlyName
    }

    init(macAddress: String, friendlyName: String = "") {
        self.macAddress = Self.normalizeMac(macAddress)
        self.friendlyName = friendlyName
    }

    private static func normalizeMac(_ input: String) -> String {
        input.replacingOccurrences(of: ":", with: "")
            .replacingOccurrences(of: "-", with: "")
            .uppercased()
    }
}
