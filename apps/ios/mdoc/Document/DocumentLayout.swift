import SwiftUI

/// Where Identities shows (ADR 0103, ADR 0104).
enum IdentitiesPlacement: Equatable {
    /// iPhone (a compact window): the drawer from the right edge.
    case drawer
    /// A regular window: the inspector beside the text.
    case inspector
    /// A narrow iPad window with a right pane (the original or the reply):
    /// Identities take that pane's place.
    case column
}

/// How the window lays the app out, from its size class and width, in one
/// place (ADR 0104): whether the sidebar stands beside the screen or slides
/// over it, and where a document's Identities show. Views derive what they
/// show from it; the user's intent lives in the models.
struct DocumentLayout: Equatable {
    enum Width: Equatable {
        /// iPhone, Slide Over, a third of a split iPad.
        case compact
        /// A regular window narrower than `narrowWindow` (iPad portrait).
        case narrow
        /// A regular window from `narrowWindow` (iPad landscape).
        case wide
    }

    var width: Width

    /// Narrower than every iPad in landscape, wider than every iPad in portrait.
    static let narrowWindow: CGFloat = 1100

    static let compact = DocumentLayout(width: .compact)
    static let narrow = DocumentLayout(width: .narrow)
    static let wide = DocumentLayout(width: .wide)

    init(width: Width) {
        self.width = width
    }

    /// A window of this size class and width. Before the width is known (0)
    /// a regular window counts as wide.
    init(sizeClass: UserInterfaceSizeClass?, width: CGFloat) {
        if sizeClass == .compact {
            self.width = .compact
        } else if width > 0 && width < Self.narrowWindow {
            self.width = .narrow
        } else {
            self.width = .wide
        }
    }

    /// iPad's side-by-side panes and inspector.
    var isRegular: Bool { width != .compact }

    /// The sidebar is a standing first column; below the wide width it is
    /// the iPhone's drawer (ADR 0102, Q13).
    var sidebarStands: Bool { width == .wide }

    /// Where Identities shows for a document with or without a right pane.
    func identities(hasRightPane: Bool) -> IdentitiesPlacement {
        switch width {
        case .compact: .drawer
        case .narrow where hasRightPane: .column
        case .narrow, .wide: .inspector
        }
    }
}

extension EnvironmentValues {
    /// The window's layout, from the root.
    @Entry var documentLayout: DocumentLayout = .compact
}
