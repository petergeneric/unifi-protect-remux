import Foundation

enum FileStatus: Sendable {
    case pending
    case processing
    case completed
    case failed
}
