import SwiftUI

/// Anything tappable that is not a button is a chip: a filled capsule. A
/// chip that opens a menu ends in a small chevron; plain text never acts
/// (ADR 0098, docs/mobile-ui.md). Use it as the label of a `Button` or a
/// `Menu`.
struct Chip: View {
    var title: Text
    var systemImage: String?
    /// Category or state color; nil is the neutral fill.
    var tint: Color?
    var opensMenu = false
    var monospaced = false

    init(_ title: Text, systemImage: String? = nil, tint: Color? = nil, opensMenu: Bool = false, monospaced: Bool = false) {
        self.title = title
        self.systemImage = systemImage
        self.tint = tint
        self.opensMenu = opensMenu
        self.monospaced = monospaced
    }

    init(_ title: String, systemImage: String? = nil, tint: Color? = nil, opensMenu: Bool = false, monospaced: Bool = false) {
        self.init(Text(verbatim: title), systemImage: systemImage, tint: tint, opensMenu: opensMenu, monospaced: monospaced)
    }

    var body: some View {
        HStack(spacing: Space.xxs) {
            if let systemImage {
                Image(systemName: systemImage)
                    .imageScale(.small)
                    .accessibilityHidden(true)
            }
            title
                .font(monospaced ? Typography.alias : Typography.chip)
                .lineLimit(1)
            if opensMenu {
                Image(systemName: "chevron.down")
                    .font(.caption2.weight(.semibold))
                    .opacity(0.7)
                    .accessibilityHidden(true)
            }
        }
        .foregroundStyle(tint ?? .primary)
        .padding(.horizontal, Space.s)
        .frame(minHeight: Size.chip)
        .background((tint ?? .primary).opacity(tint == nil ? 0.08 : 0.15), in: Capsule())
        .contentShape(Capsule())
        // The whole chip is the target, at least 44 pt high.
        .frame(minHeight: Size.target)
        .contentShape(Rectangle())
    }
}

#Preview("Chips") {
    VStack(alignment: .leading, spacing: Space.s) {
        HStack {
            Chip("Person", systemImage: "person", tint: .blue, opensMenu: true)
            Chip("PERSON_1", opensMenu: true, monospaced: true)
            Chip("All 6", opensMenu: true)
        }
        HStack {
            Chip("1 of 3", systemImage: "arrow.down")
            Chip("Same as", systemImage: "link", tint: .accentColor)
        }
    }
    .padding()
}

#Preview("Chips, dark, AX3") {
    VStack(alignment: .leading, spacing: Space.s) {
        Chip("Organization", systemImage: "building.2", tint: .purple, opensMenu: true)
        Chip("ORG_1", opensMenu: true, monospaced: true)
    }
    .padding()
    .preferredColorScheme(.dark)
    .environment(\.dynamicTypeSize, .accessibility3)
}

#Preview("Chips, matrix") {
    PreviewMatrix {
        HStack {
            Chip("Person", systemImage: "person", tint: .blue, opensMenu: true)
            Chip("PERSON_1", opensMenu: true, monospaced: true)
            Chip("All 6", opensMenu: true)
        }
    }
}
