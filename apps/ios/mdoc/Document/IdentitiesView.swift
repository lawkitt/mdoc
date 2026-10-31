import SwiftUI

/// Identities by category: a sheet on iPhone, beside the text on iPad.
struct IdentitiesView: View {
    let identities: [Identity]
    let onOpen: (IdentityID) -> Void
    let onDecide: (IdentityID, Decision) -> Void
    /// Done, where the list is a sheet (iPhone); nil beside the text.
    var onClose: (() -> Void)?
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    private var groups: [(Category, [Identity])] {
        Category.allCases.compactMap { category in
            let members = identities.filter { $0.category == category }
            return members.isEmpty ? nil : (category, members)
        }
    }

    var body: some View {
        List {
            if identities.isEmpty {
                ContentUnavailableView(
                    "No names yet",
                    systemImage: "person.text.rectangle",
                    description: Text("Find names, or select words in the text and choose Replace…."))
            }
            ForEach(groups, id: \.0) { category, members in
                Section {
                    ForEach(members) { identity in
                        row(identity)
                    }
                } header: {
                    Label("\(category.label) (\(members.count))", systemImage: category.symbol)
                }
            }
        }
        .listStyle(.insetGrouped)
        // Its own title row: a navigation title here would replace the
        // document's in the bar on iPhone, where the inspector is a sheet.
        .safeAreaInset(edge: .top, spacing: 0) {
            // Title and Done side by side, the title wrapping first, so they
            // never overlap at large text sizes.
            HStack(alignment: .firstTextBaseline) {
                Text("Identities")
                    .font(.headline)
                    .frame(maxWidth: .infinity, alignment: onClose == nil ? .center : .leading)
                    .accessibilityAddTraits(.isHeader)
                if let onClose {
                    Button("Done", action: onClose)
                        .fontWeight(.semibold)
                        .fixedSize()
                        // A full 44-point target, as the toolbar's Done has.
                        .frame(minWidth: 44, minHeight: 44)
                        .contentShape(Rectangle())
                        .accessibilityIdentifier("identities-done")
                }
            }
            .frame(maxWidth: .infinity, minHeight: 44)
            .padding(.horizontal)
            .padding(.vertical, 2)
            .background(.bar)
        }
    }

    private func row(_ identity: Identity) -> some View {
        Button {
            onOpen(identity.id)
        } label: {
            // At accessibility sizes the chip goes under the value, which
            // then has the row's width.
            (dynamicTypeSize.isAccessibilitySize
                ? AnyLayout(VStackLayout(alignment: .leading, spacing: 6))
                : AnyLayout(HStackLayout(alignment: .firstTextBaseline))) {
                VStack(alignment: .leading, spacing: 2) {
                    UnhyphenatedText(identity.primaryWording)
                    // Secondary text, not the category's colour: as text that
                    // colour fails contrast (the header's symbol carries it).
                    UnhyphenatedText(identity.alias, style: .footnote, monospaced: true, color: .secondaryLabel)
                }
                if !dynamicTypeSize.isAccessibilitySize {
                    Spacer()
                }
                stateBadge(identity)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(Self.spoken(identity)))
        .accessibilityHint(Text("Opens the first mention."))
        .accessibilityAction(named: Text("Replace all")) {
            onDecide(identity.id, .replace)
        }
        .accessibilityAction(named: Text("Keep all")) {
            onDecide(identity.id, .keep)
        }
        .swipeActions(edge: .trailing) {
            Button("Keep") {
                onDecide(identity.id, .keep)
            }
            .tint(.gray)
            Button("Replace") {
                onDecide(identity.id, .replace)
            }
            .tint(identity.category.color)
        }
    }

    private func stateBadge(_ identity: Identity) -> some View {
        let text: String
        let symbol: String
        switch identity.state {
        case .proposed:
            text = String(localized: "Proposed")
            symbol = "questionmark.circle"
        case .replaced:
            text = String(localized: "Replaced")
            symbol = "checkmark.circle.fill"
        case .kept:
            text = String(localized: "Kept")
            symbol = "minus.circle"
        }
        // A labelled chip; the words carry the state, not the colour.
        return HStack(spacing: 4) {
            Image(systemName: symbol)
                .imageScale(.small)
            Text("\(text) · \(identity.mentionCount)")
        }
        .font(.caption.weight(.semibold))
        // Text colour for every state: secondary text on the fill fails
        // contrast, and the words already say which state it is. A concrete
        // colour, so the row button's tint doesn't take it over.
        .foregroundStyle(Color.primary)
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(Color(uiColor: .tertiarySystemFill), in: Capsule())
    }

    /// "Maya Ellison, Person, PERSON_1, 4 mentions, proposed".
    static func spoken(_ identity: Identity) -> String {
        let state = switch identity.state {
        case .proposed: String(localized: "proposed")
        case .replaced: String(localized: "replaced")
        case .kept: String(localized: "kept")
        }
        let mentions = String(localized: "\(identity.mentionCount) mentions")
        return [identity.primaryWording, identity.category.label, identity.alias, mentions, state].joined(separator: ", ")
    }
}
