import SwiftUI
import UIKit

/// An alias as the document shows a replaced name: monospaced, the text's
/// own colour on its category's tint (`Category.aliasFill`). One line, so
/// it is never broken or hyphenated.
struct AliasTag: View {
    let alias: String
    let category: Category

    var body: some View {
        Text(verbatim: alias)
            .font(Typography.alias)
            .foregroundStyle(.primary)
            .lineLimit(1)
            .padding(.horizontal, Space.xxs)
            .background(Color(uiColor: category.aliasFill), in: RoundedRectangle(cornerRadius: Radius.tag, style: .continuous))
            .fixedSize()
    }
}

/// A category's symbol in its colour as a system menu row's icon
/// (`Category.menuIcon`).
struct MenuIcon: View {
    let category: Category

    var body: some View {
        if let icon = category.menuIcon {
            Image(uiImage: icon)
        } else {
            Image(systemName: category.symbol)
        }
    }
}

extension String {
    /// This alias for a system menu's row, which wraps and hyphenates its
    /// title as it likes ("PERSO-N_6"): a word joiner between every two
    /// characters leaves it no place to break. For display only. A long
    /// alias is left to wrap: whole, a narrow menu would cut its end off.
    var unbrokenAlias: String {
        count > Self.unbrokenAliasLimit ? self : map(String.init).joined(separator: "\u{2060}")
    }

    /// The core's own aliases stay under it ("IDENTITY_12"); "ADDRESS_4"
    /// fills most of a menu row's line at the largest standard text size.
    static let unbrokenAliasLimit = 12
}

#Preview("Alias tags") {
    VStack(alignment: .leading, spacing: Space.s) {
        ForEach(Category.allCases, id: \.self) { category in
            AliasTag(alias: "\(category.label.uppercased())_1", category: category)
        }
    }
    .padding()
}
