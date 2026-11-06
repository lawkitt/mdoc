import SwiftUI
import UIKit

extension View {
    /// A short sheet's heights on an iPhone at ordinary text sizes: `first`
    /// (half the screen unless the sheet asks for more), then full. At
    /// accessibility sizes half the screen holds a line or two, and on iPad a
    /// medium detent draws a short card that cuts off the sheet's last rows,
    /// so there it opens at full height.
    func reviewSheetDetents(first: PresentationDetent = .medium) -> some View {
        modifier(ReviewSheetDetents(first: first))
    }
}

private struct ReviewSheetDetents: ViewModifier {
    let first: PresentationDetent
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    func body(content: Content) -> some View {
        let tall = dynamicTypeSize.isAccessibilitySize || UIDevice.current.userInterfaceIdiom == .pad
        content.presentationDetents(tall ? [.large] : [first, .large])
    }
}
