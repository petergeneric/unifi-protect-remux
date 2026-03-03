import Foundation

@Observable
final class CameraEntry: Identifiable {
    let id = UUID()
    var macAddress: String
    var friendlyName: String

    var macAddressFormatted: String {
        guard macAddress.count == 12 else { return macAddress }
        return stride(from: 0, to: 12, by: 2).map { i in
            let start = macAddress.index(macAddress.startIndex, offsetBy: i)
            let end = macAddress.index(start, offsetBy: 2)
            return String(macAddress[start..<end])
        }.joined(separator: ":")
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
