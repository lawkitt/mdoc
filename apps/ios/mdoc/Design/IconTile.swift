import SwiftUI

/// A white symbol on a colored rounded square, as in iOS Settings; Home
/// uses a tinted variant for a document's source.
struct IconTile: View {
    var systemImage: String
    var color: Color
    /// Tinted: a light fill with the symbol in the color (Home rows).
    var tinted = false
    var size: CGFloat = Size.tile

    var body: some View {
        Image(systemName: systemImage)
            .font(.system(size: size * 0.5, weight: .semibold))
            .foregroundStyle(tinted ? color : .white)
            .frame(width: size, height: size)
            .background(tinted ? color.opacity(0.15) : color, in: RoundedRectangle(cornerRadius: Radius.tile, style: .continuous))
            .accessibilityHidden(true)
    }
}

#Preview("Icon tiles") {
    HStack(spacing: Space.s) {
        IconTile(systemImage: "paintbrush", color: .blue)
        IconTile(systemImage: "text.magnifyingglass", color: .indigo)
        IconTile(systemImage: "lock", color: .green)
        IconTile(systemImage: "doc.richtext", color: .red, tinted: true, size: 40)
        IconTile(systemImage: "doc.viewfinder", color: .teal, tinted: true, size: 40)
    }
    .padding()
}

#Preview("Icon tiles, matrix") {
    PreviewMatrix {
        HStack(spacing: Space.s) {
            IconTile(systemImage: "paintbrush", color: .blue)
            IconTile(systemImage: "lock", color: .green)
            IconTile(systemImage: "doc.richtext", color: .red, tinted: true, size: 40)
        }
    }
}
