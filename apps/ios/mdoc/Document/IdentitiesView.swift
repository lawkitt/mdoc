import SwiftUI

/// Identities by category: a sheet on iPhone, beside the text on iPad.
struct IdentitiesView: View {
    let identities: [Identity]
    let onOpen: (IdentityID) -> Void
    let onDecide: (IdentityID, Decision) -> Void

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
        .navigationTitle("Identities")
    }

    private func row(_ identity: Identity) -> some View {
        Button {
            onOpen(identity.id)
        } label: {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(identity.primaryWording)
                        .foregroundStyle(.primary)
                    Text(identity.alias)
                        .font(.footnote.monospaced())
                        .foregroundStyle(identity.category.color)
                }
                Spacer()
                stateBadge(identity)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(Self.spoken(identity)))
        .accessibilityHint(Text("Opens the first mention."))
        .accessibilityAction(named: Text("Replace all")) {
            onDecide(identity.id, .replace(alias: identity.alias))
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
                onDecide(identity.id, .replace(alias: identity.alias))
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
            symbol = "circle.dashed"
        case .replaced:
            text = String(localized: "Replaced")
            symbol = "checkmark.circle.fill"
        case .kept:
            text = String(localized: "Kept")
            symbol = "minus.circle"
        }
        return Label {
            Text("\(text) · \(identity.mentionCount)")
        } icon: {
            Image(systemName: symbol)
        }
        .font(.footnote)
        .foregroundStyle(.secondary)
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
