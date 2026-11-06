import SwiftUI

/// A document's or a mention's state as an icon and words in the state's
/// color: to review (orange), ready (green), restored (blue), or neutral.
struct StatusPill: View {
    enum Kind: Sendable {
        case toReview, ready, restored, neutral

        var color: Color {
            switch self {
            case .toReview: Palette.toReview
            case .ready: Palette.ready
            case .restored: Palette.restored
            case .neutral: Palette.secondaryText
            }
        }
    }

    var text: Text
    var systemImage: String
    var kind: Kind

    init(_ text: Text, systemImage: String, kind: Kind) {
        self.text = text
        self.systemImage = systemImage
        self.kind = kind
    }

    init(_ text: String, systemImage: String, kind: Kind) {
        self.init(Text(verbatim: text), systemImage: systemImage, kind: kind)
    }

    var body: some View {
        HStack(spacing: Space.xxs) {
            Image(systemName: systemImage)
                .imageScale(.small)
                .accessibilityHidden(true)
            text
        }
        .font(Typography.caption.weight(kind == .toReview ? .semibold : .regular))
        .foregroundStyle(kind.color)
    }
}

#Preview("Status pills") {
    VStack(alignment: .leading, spacing: Space.xs) {
        StatusPill("11 to review", systemImage: "exclamationmark.circle", kind: .toReview)
        StatusPill("Ready to share", systemImage: "checkmark.circle", kind: .ready)
        StatusPill("Reply restored", systemImage: "arrow.uturn.backward.circle", kind: .restored)
        StatusPill("Recognizing text", systemImage: "text.viewfinder", kind: .neutral)
    }
    .padding()
}

#Preview("Status pills, dark") {
    VStack(alignment: .leading, spacing: Space.xs) {
        StatusPill("11 to review", systemImage: "exclamationmark.circle", kind: .toReview)
        StatusPill("Ready to share", systemImage: "checkmark.circle", kind: .ready)
    }
    .padding()
    .preferredColorScheme(.dark)
}

#Preview("Status pills, matrix") {
    PreviewMatrix {
        VStack(alignment: .leading, spacing: Space.xs) {
            StatusPill("11 to review", systemImage: "exclamationmark.circle", kind: .toReview)
            StatusPill("Ready to share", systemImage: "checkmark.circle", kind: .ready)
            StatusPill("Reply restored", systemImage: "arrow.uturn.backward.circle", kind: .restored)
        }
    }
}
