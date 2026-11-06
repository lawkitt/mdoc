import UIKit

extension AppTheme {
    /// Applies the theme to every window of the app, so sheets, menus and
    /// alerts follow it as well as the SwiftUI tree. App only: extensions
    /// have no `UIApplication.shared`.
    @MainActor
    func apply() {
        for scene in UIApplication.shared.connectedScenes {
            guard let scene = scene as? UIWindowScene else { continue }
            for window in scene.windows {
                window.overrideUserInterfaceStyle = interfaceStyle
            }
        }
    }
}
