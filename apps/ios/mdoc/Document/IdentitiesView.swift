import SwiftUI

/// Identities by category: a sheet on iPhone, beside the text on iPad.
struct IdentitiesView: View {
    /// Which identities the list shows (ADR 0098's Show filter).
    enum Show: String, CaseIterable, Identifiable {
        case all, proposed, replaced, kept
        var id: String { rawValue }
        var title: LocalizedStringKey {
            switch self {
            case .all: "All"
            case .proposed: "Proposed"
            case .replaced: "Replaced"
            case .kept: "Kept"
            }
        }
        func includes(_ state: MentionState) -> Bool {
            switch self {
            case .all: true
            case .proposed: state == .proposed
            case .replaced: state == .replaced
            case .kept: state == .kept
            }
        }
    }

    let identities: [Identity]
    let onOpen: (IdentityID) -> Void
    let onDecide: (IdentityID, Decision) -> Void
    /// Review this identity, or Review 3 people (focus).
    var onFocus: (ReviewFocus) -> Void = { _ in }
    /// Same as, for a whole identity.
    var onSameAs: (IdentityID, IdentityID) -> Void = { _, _ in }
    /// Same as… from a row: the shared picker, hints first.
    var onPickSameAs: (IdentityID) -> Void = { _ in }
    /// Each identity's mentions with their context, for its mention rows.
    var mentions: (IdentityID) -> [(id: CandidateID, context: String, state: MentionState)] = { _ in [] }
    var onOpenMention: (CandidateID) -> Void = { _ in }
    /// Done, where the list is a sheet (iPhone); nil beside the text.
    var onClose: (() -> Void)?
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var query = ""
    @State private var show: Show = .all
    @State private var expanded: Set<IdentityID> = []
    @State private var sameAsSource: Identity?

    private var shown: [Identity] {
        let needle = query.trimmingCharacters(in: .whitespaces)
        return identities.filter { identity in
            show.includes(identity.state)
                && (needle.isEmpty || identity.alias.localizedStandardContains(needle)
                    || identity.variants.contains { $0.localizedStandardContains(needle) }
                    || identity.primaryWording.localizedStandardContains(needle))
        }
    }

    private var groups: [(Category, [Identity])] {
        Category.allCases.compactMap { category in
            let members = shown.filter { $0.category == category }
            return members.isEmpty ? nil : (category, members)
        }
    }

    var body: some View {
        List {
            if identities.isEmpty {
                ContentUnavailableView(
                    "No names yet",
                    systemImage: "person.text.rectangle",
                    description: Text("Find names, or select words in the text and choose Hide name."))
            }
            if identities.count > 1 {
                // In the list, not the navigation bar: beside the text on
                // iPad the bar belongs to the document.
                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                    TextField("Names and aliases", text: $query)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .accessibilityIdentifier("identities-search")
                    if !query.isEmpty {
                        Button {
                            query = ""
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.secondary)
                        }
                        .accessibilityLabel(Text("Clear"))
                    }
                }
                Picker("Show", selection: $show) {
                    ForEach(Show.allCases) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets())
                .accessibilityIdentifier("identities-show")
            }
            ForEach(groups, id: \.0) { category, members in
                Section {
                    ForEach(members) { identity in
                        row(identity)
                        if expanded.contains(identity.id) {
                            ForEach(mentions(identity.id), id: \.id) { mention in
                                mentionRow(mention)
                            }
                        }
                    }
                } header: {
                    HStack {
                        Label("\(category.label) (\(members.count))", systemImage: category.symbol)
                        Spacer()
                        let proposed = members.filter { $0.state == .proposed }.count
                        if proposed > 0 {
                            Button("Review \(proposed)") { onFocus(.category(category)) }
                                .font(.caption.weight(.semibold))
                                .textCase(nil)
                                .accessibilityLabel(Text("Review \(proposed) \(category.label)"))
                                .accessibilityIdentifier("identities-review-\(category.rawValue)")
                        }
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
        .onChange(of: sameAsSource) { _, source in
            if let source {
                onPickSameAs(source.id)
                sameAsSource = nil
            }
        }
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
                    // How often it appears, when more than once; the chip
                    // says only what happens to it.
                    if identity.mentionCount > 1 {
                        Text("\(identity.mentionCount) mentions")
                            .font(.footnote)
                            // A concrete colour: the row button's tint would
                            // take over `.secondary`.
                            .foregroundStyle(Color(uiColor: .secondaryLabel))
                    }
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
        // Drag one row onto another: the same name (ADR 0098, iPad first).
        .draggable(identity.id.rawValue)
        .dropDestination(for: String.self) { items, _ in
            guard let dropped = items.first, dropped != identity.id.rawValue,
                  identities.contains(where: { $0.id.rawValue == dropped }) else { return false }
            onSameAs(IdentityID(rawValue: dropped), identity.id)
            return true
        }
        .accessibilityAction(named: Text("Review this identity")) { onFocus(.identity(identity.id)) }
        .accessibilityAction(named: Text("Same as…")) { sameAsSource = identity }
        // Right: Replace. Left: Keep, and Same as… (ADR 0098).
        .swipeActions(edge: .leading) {
            Button("Replace") {
                onDecide(identity.id, .replace)
            }
            .tint(Palette.accent)
        }
        .swipeActions(edge: .trailing) {
            Button("Keep") {
                onDecide(identity.id, .keep)
            }
            .tint(.gray)
            Button("Same as…") {
                sameAsSource = identity
            }
            .tint(.indigo)
        }
        .contextMenu {
            Button {
                onFocus(.identity(identity.id))
            } label: {
                Label("Review this identity", systemImage: "rectangle.stack")
            }
            Button {
                onDecide(identity.id, .replace)
            } label: {
                Label("Replace all", systemImage: "eye.slash")
            }
            Button {
                onDecide(identity.id, .keep)
            } label: {
                Label("Keep all", systemImage: "eye")
            }
            Button {
                sameAsSource = identity
            } label: {
                Label("Same as…", systemImage: "link")
            }
            Button {
                if expanded.contains(identity.id) { expanded.remove(identity.id) } else { expanded.insert(identity.id) }
            } label: {
                Label(expanded.contains(identity.id) ? "Hide mentions" : "Show mentions", systemImage: "list.bullet.indent")
            }
        }
    }

    /// One mention under its identity: its context and state; a tap opens
    /// the bar on it.
    private func mentionRow(_ mention: (id: CandidateID, context: String, state: MentionState)) -> some View {
        Button {
            onOpenMention(mention.id)
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: ReviewDetails.symbol(mention.state))
                    .foregroundStyle(.secondary)
                    .accessibilityLabel(Text(ReviewDetails.stateName(mention.state)))
                Text(mention.context)
                    .font(.footnote)
                    .foregroundStyle(Color(uiColor: .label))
                    .lineLimit(2)
            }
            .padding(.leading, 12)
        }
        .accessibilityIdentifier("identities-mention")
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
            Text(text)
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
