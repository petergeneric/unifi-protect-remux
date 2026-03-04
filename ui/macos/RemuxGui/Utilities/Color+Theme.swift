import SwiftUI

extension Color {
    // MARK: - Status
    static let statusPending = Color(nsColor: .tertiaryLabelColor)
    static let statusProcessing = Color(nsColor: .systemOrange)
    static let statusCompleted = Color(nsColor: .systemGreen)
    static let statusFailed = Color(nsColor: .systemRed)

    // MARK: - Log levels
    static let logError = Color(nsColor: .systemRed)
    static let logWarn = Color(nsColor: .systemOrange)
    static let logInfo = Color.secondary

    // MARK: - UI
    static let navInactiveFg = Color(nsColor: .tertiaryLabelColor)
    static let cardBackgroundAlt = Color(nsColor: .windowBackgroundColor)
}
