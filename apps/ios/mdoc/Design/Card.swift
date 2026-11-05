import SwiftUI

/// A raised surface: the review card, Home's empty-state tiles, a
/// grouped block outside a List.
struct CardStyle: ViewModifier {
    var padding: CGFloat = Space.m

    func body(content: Content) -> some View {
        content
            .padding(padding)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
    }
}

extension View {
    func card(padding: CGFloat = Space.m) -> some View {
        modifier(CardStyle(padding: padding))
    }
}

#Preview("Card") {
    VStack(alignment: .leading, spacing: Space.xs) {
        Text("Riverside Holdings").font(Typography.title)
        Text("Organization · ORG_1").font(Typography.caption).foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .card()
    .padding()
    .background(Palette.background)
}

#Preview("Card, dark") {
    Text("Riverside Holdings")
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
        .padding()
        .background(Palette.background)
        .preferredColorScheme(.dark)
}

#Preview("Card, matrix") {
    PreviewMatrix {
        VStack(alignment: .leading, spacing: Space.xs) {
            Text(PreviewFixtures.sample.names[0].wording).font(Typography.title)
            Text(verbatim: "Organization · ORG_1").font(Typography.caption).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
    }
}
