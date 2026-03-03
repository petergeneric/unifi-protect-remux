import Foundation

struct LicenseEntry: Decodable, Identifiable {
    var id: String { name }
    let name: String
    let version: String
    let license: String
    let authors: String
    let repository: String
}
