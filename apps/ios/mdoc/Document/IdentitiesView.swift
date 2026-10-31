import SwiftUI

/// Identities by category: a sheet on iPhone, beside the text on iPad.
struct IdentitiesView: View {
    let identities: [Identity]
    let onOpen: (IdentityID) -> Void
    let onDecide: (IdentityID, Decision) -> Void
    /// Done, where the list is a sheet (iPhone); nil beside the text.
    var onClose: (() -> Void)?

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
            ZStack {
                Text("Identities")
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                if let onClose {
                    HStack {
                        Spacer()
                        Button("Done", action: onClose)
                            .fontWeight(.semibold)
                            .accessibilityIdentifier("identities-done")
                    }
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal)
            .padding(.vertical, 12)
            .background(.bar)
        }
    }

    private func row(_ identity: Identity) -> some View {
        Button {
            onOpen(identity.id)
        } label: {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 2) {
                    UnhyphenatedText(identity.primaryWording)
                    UnhyphenatedText(identity.alias, style: .footnote, monospaced: true, color: identity.category.uiColor)
                }
                Spacer()
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
        .foregroundStyle(identity.state == .proposed ? .primary : .secondary)
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
