import SwiftUI
import UIKit

/// An alias as the document shows a replaced name: monospaced, the text's
/// own colour on its category's tint (`Category.aliasFill`). One line and
/// never hyphenated: an alias too long for its row (the core allows 128
/// characters) is cut in the middle, so its start and its number both
/// show. VoiceOver hears it whole.
struct AliasTag: View {
    let alias: String
    let category: Category

    var body: some View {
        Text(verbatim: alias)
            .font(Typography.alias)
            .foregroundStyle(.primary)
            .lineLimit(1)
            .truncationMode(.middle)
            .padding(.horizontal, Space.xxs)
            .background(Color(uiColor: category.aliasFill), in: RoundedRectangle(cornerRadius: Radius.tag, style: .continuous))
            .accessibilityLabel(Text(verbatim: alias))
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
    /// title as it likes ("PERSO-N_6"). For display only. A word joiner
    /// between every two characters leaves a short alias no place to
    /// break. A long one (past `unbrokenAliasLimit`) may break after a
    /// separator ("CLIENT_/HOLDING_/COMPANY_2"), since whole it would be
    /// cut off in a narrow menu; it never breaks inside a word.
    var unbrokenAlias: String {
        let whole = count <= Self.unbrokenAliasLimit
        var display = ""
        var rest = self[...]
        while let character = rest.popFirst() {
            display.append(character)
            guard !rest.isEmpty else { break }
            let separator = !(character.isLetter || character.isNumber)
            display.append(separator && !whole ? "\u{200B}" : "\u{2060}")
        }
        return display
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

#Preview("Alias tags, matrix") {
    PreviewMatrix {
        VStack(alignment: .leading, spacing: Space.xs) {
            AliasTag(alias: "PERSON_2", category: .person)
            AliasTag(alias: "ORG_1", category: .organization)
            AliasTag(alias: "CLIENT_HOLDING_COMPANY_REGISTERED_IN_ENGLAND_AND_WALES_2", category: .organization)
        }
    }
}
#Preview("Alias tags") {
    VStack(alignment: .leading, spacing: Space.s) {
        ForEach(Category.allCases, id: \.self) { category in
            AliasTag(alias: "\(category.label.uppercased())_1", category: category)
        }
    }
    .padding()
}
