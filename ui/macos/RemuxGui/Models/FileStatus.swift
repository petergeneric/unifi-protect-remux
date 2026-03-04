import SwiftUI

enum FileStatus: Sendable {
    case pending
    case processing
    case completed
    case failed

    var color: Color {
        switch self {
        case .pending: .statusPending
        case .processing: .statusProcessing
        case .completed: .statusCompleted
        case .failed: .statusFailed
        }
    }
}
