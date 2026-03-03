import Foundation

struct ProgressEvent: Decodable {
    let type: String
    let level: String?
    let message: String?
    let path: String?
    let count: Int?
    let index: Int?
    let total: Int?
    let error: String?
    let outputs: [String]?
    let errors: [String]?
}
