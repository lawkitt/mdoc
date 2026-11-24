import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// The system's Paste control as a large add tile or a bar button: one tap
/// pastes without iOS's "Allow Paste" prompt, which only the system's own
/// control skips. Its words and symbol are the system's, so a look sets only
/// its colours and corners; it dims while the clipboard holds nothing of
/// `types`.
struct PasteControl: UIViewRepresentable {
    enum Look {
        /// Start's add tiles: the card's surface and corners.
        case tile
        /// A big secondary button: ink words on the card's surface, which
        /// stands in for glass, because the control draws a clear
        /// background as black.
        case bar
    }

    let look: Look
    let types: [UTType]
    let onPaste: ([NSItemProvider]) -> Void

    func makeCoordinator() -> Target { Target() }

    func makeUIView(context: Context) -> UIPasteControl {
        let configuration = UIPasteControl.Configuration()
        configuration.displayMode = .iconAndLabel
        switch look {
        case .tile:
            configuration.cornerStyle = .fixed
            configuration.cornerRadius = Radius.card
            configuration.baseBackgroundColor = .secondarySystemGroupedBackground
            configuration.baseForegroundColor = .label
        case .bar:
            configuration.cornerStyle = .capsule
            configuration.baseBackgroundColor = .secondarySystemGroupedBackground
            configuration.baseForegroundColor = .ink
        }
        // What it takes is set before it has a target, so it is enabled
        // from the start when the clipboard already holds text.
        context.coordinator.pasteConfiguration = UIPasteConfiguration(acceptableTypeIdentifiers: types.map(\.identifier))
        let control = UIPasteControl(configuration: configuration)
        control.target = context.coordinator
        return control
    }

    func updateUIView(_ control: UIPasteControl, context: Context) {
        context.coordinator.onPaste = onPaste
    }

    /// The width it is offered, so it fills its tile or its share of a row;
    /// its own width when asked for its least or its ideal. Its height is
    /// its look's, as tall as the add tiles beside it or a big button, and
    /// never what a container offers, which can be the whole screen.
    func sizeThatFits(_ proposal: ProposedViewSize, uiView: UIPasteControl, context: Context) -> CGSize? {
        let natural = uiView.intrinsicContentSize
        let width = proposal.width.flatMap { $0 > 0 && $0.isFinite ? $0 : nil } ?? natural.width
        let least: CGFloat = switch look {
        case .tile: AddTile.height
        case .bar: BigButton.height
        }
        return CGSize(width: width, height: max(least, natural.height))
    }

    /// What the control pastes into: it asks `pasteConfiguration` whether
    /// the clipboard has anything to take.
    final class Target: UIResponder {
        var onPaste: ([NSItemProvider]) -> Void = { _ in }

        override func paste(itemProviders: [NSItemProvider]) {
            onPaste(itemProviders)
        }
    }
}
