import UIKit

extension AppTheme {
    /// Applies the theme to every window of the app, so sheets, menus and
    /// alerts follow it as well as the SwiftUI tree. A sheet already open
    /// keeps the style it was presented with until it is told, so each
    /// presented controller is set too: Settings follows the theme picked
    /// in it at once (#1036). App only: extensions have no
    /// `UIApplication.shared`.
    @MainActor
    func apply() {
        for scene in UIApplication.shared.connectedScenes {
            guard let scene = scene as? UIWindowScene else { continue }
            for window in scene.windows {
                window.overrideUserInterfaceStyle = interfaceStyle
                var presented = window.rootViewController?.presentedViewController
                while let controller = presented {
                    controller.overrideUserInterfaceStyle = interfaceStyle
                    presented = controller.presentedViewController
                }
            }
        }
    }
}
