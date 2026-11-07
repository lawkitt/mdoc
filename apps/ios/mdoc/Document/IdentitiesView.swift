import SwiftUI

/// Identities (2026-11-06 grill, round 2; docs/mobile-ui.md, Identities):
/// the document's names, a drawer from the right edge on iPhone and the
/// standing right column on iPad. At the top, how many names and how many
/// are left, Replace N and search; then Looks like cards, a category filter,
/// and the names by what is left: To review, Replaced, Kept. Each row's
/// round button replaces or keeps that name; a tap opens the review bar's
/// full view on it; Select acts on several at once.
struct IdentitiesView: View {
    @Bindable var model: DocumentViewModel
    /// The bar's full view on a name (and the drawer closed on iPhone).
    let onOpen: (IdentityID) -> Void
    /// Review this identity or a category (focus), from a row's menu.
    var onFocus: (ReviewFocus) -> Void = { _ in }
    /// Same as… for one identity: the shared picker, hints first.
    var onPickSameAs: (IdentityID) -> Void = { _ in }
    /// × where the list is a drawer (iPhone); nil beside the text.
    var onClose: (() -> Void)?

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var query = ""
    @State private var searching = false
    @FocusState private var searchFocused: Bool
    @State private var category: Category?
    @State private var selecting = false
    @State private var selection: Set<IdentityID> = []
    @State private var showsKept = false

    private var identities: [Identity] { model.snapshot.identities }

    private var shown: [Identity] {
        let needle = query.trimmingCharacters(in: .whitespaces)
        return identities.filter { identity in
            (category == nil || identity.category == category)
                && (needle.isEmpty || identity.alias.localizedStandardContains(needle)
                    || identity.variants.contains { $0.localizedStandardContains(needle) }
                    || identity.primaryWording.localizedStandardContains(needle))
        }
    }

    private var categories: [(Category, Int)] {
        Category.allCases.compactMap { category in
            let count = identities.filter { $0.category == category }.count
            return count == 0 ? nil : (category, count)
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            List {
                if identities.isEmpty {
                    ContentUnavailableView(
                        "No names yet",
                        systemImage: "person.text.rectangle",
                        description: Text("Find names, or select words in the text and choose Hide name."))
                        .listRowBackground(Color.clear)
                }
                if !selecting && query.isEmpty {
                    hints
                }
                if categories.count > 1 {
                    categoryChips
                        .listRowInsets(EdgeInsets(top: Space.xs, leading: Space.m, bottom: Space.xs, trailing: 0))
                        .listRowBackground(Color.clear)
                        .listRowSeparator(.hidden)
                }
                section("To review", shown.filter { $0.state == .proposed })
                section("Replaced", shown.filter { $0.state == .replaced })
                keptSection(shown.filter { $0.state == .kept })
            }
            .listStyle(.plain)
            .listSectionSpacing(Space.xs)
            .scrollContentBackground(.hidden)
            .contentMargins(.bottom, selecting ? 72 : Space.m, for: .scrollContent)
        }
        .background(Palette.background)
        .overlay(alignment: .bottom) {
            if selecting { selectionBar }
        }
        .animation(Motion.snappy, value: selecting)
        .animation(Motion.snappy, value: searching)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("identities-panel")
    }

    // MARK: Header

    /// "15 names · 4 to review" beside a ring; Replace N; search and Select.
    private var header: some View {
        let counts = model.identityCounts
        return VStack(alignment: .leading, spacing: Space.s) {
            HStack(spacing: Space.s) {
                ProgressRing(fraction: counts.total == 0 ? 1 : Double(counts.total - counts.toReview) / Double(counts.total))
                VStack(alignment: .leading, spacing: 0) {
                    Text("Identities")
                        .font(Typography.title)
                        .accessibilityAddTraits(.isHeader)
                    Text(summary(counts))
                        .font(Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer(minLength: 0)
                circleButton(searching ? "xmark" : "magnifyingglass", label: searching ? "Cancel search" : "Search names") {
                    searching.toggle()
                    if searching { searchFocused = true } else { query = "" }
                }
                .accessibilityIdentifier("identities-search-button")
                if !identities.isEmpty {
                    Button(selecting ? "Done" : "Select") {
                        selecting.toggle()
                        selection = []
                    }
                    .font(Typography.chip)
                    .frame(minWidth: Size.target, minHeight: Size.target)
                    .accessibilityIdentifier("identities-select")
                }
                if let onClose {
                    circleButton("chevron.right", label: "Close Identities", action: onClose)
                        .accessibilityIdentifier("identities-done")
                }
            }
            if searching {
                HStack(spacing: Space.xs) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                    TextField("Names and aliases", text: $query)
                        .focused($searchFocused)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .accessibilityIdentifier("identities-search")
                }
                .padding(.horizontal, Space.s)
                .frame(minHeight: Size.target)
                .background(Palette.chipFill, in: Capsule())
                .transition(.opacity)
            }
            if counts.toReview > 0 && !selecting {
                Button {
                    model.replaceAllPending()
                } label: {
                    Label("Replace \(counts.toReview)", systemImage: "eye.slash")
                        .fontWeight(.semibold)
                        .frame(maxWidth: .infinity, minHeight: Size.barButton)
                }
                .buttonStyle(.borderedProminent)
                .accessibilityLabel(Text("Replace all \(counts.toReview) names left"))
                .accessibilityIdentifier("identities-replace-all")
            }
        }
        .padding(.horizontal, Space.m)
        .padding(.top, Space.s)
        .padding(.bottom, Space.xs)
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
    }

    private func summary(_ counts: (total: Int, toReview: Int)) -> String {
        let names = counts.total == 1 ? String(localized: "1 name") : String(localized: "\(counts.total) names")
        guard counts.toReview > 0 else { return names + " · " + String(localized: "all decided") }
        return names + " · " + String(localized: "\(counts.toReview) left")
    }

    private func circleButton(_ symbol: String, label: LocalizedStringKey, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.primary)
                .frame(width: 34, height: 34)
                .background(Palette.chipFill, in: Circle())
                .frame(width: Size.target, height: Size.target)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(label))
    }

    // MARK: Looks like

    @ViewBuilder
    private var hints: some View {
        let hints = model.identityHints
        if !hints.isEmpty {
            Section {
                ForEach(hints, id: \.hint) { item in
                    HintCard(from: item.from, into: item.into) {
                        model.link(item.from.id, into: item.into.id)
                    } onDismiss: {
                        model.dismissHint(item.hint)
                    }
                    .listRowInsets(EdgeInsets(top: Space.xxs, leading: Space.m, bottom: Space.xxs, trailing: Space.m))
                    .listRowBackground(Color.clear)
                    .listRowSeparator(.hidden)
                }
            } header: {
                sectionHeader("Looks like", count: nil)
            }
        }
    }

    // MARK: Categories

    private var categoryChips: some View {
        ScrollView(.horizontal) {
            HStack(spacing: Space.xs) {
                Button {
                    category = nil
                } label: {
                    Chip(Text("All"), tint: category == nil ? Palette.accent : nil)
                }
                ForEach(categories, id: \.0) { item in
                    Button {
                        category = category == item.0 ? nil : item.0
                    } label: {
                        Chip(Text("\(item.0.label) \(item.1)"), systemImage: item.0.symbol, tint: category == item.0 ? item.0.color : nil)
                    }
                    .accessibilityAddTraits(category == item.0 ? .isSelected : [])
                }
            }
            .buttonStyle(.plain)
            .padding(.trailing, Space.m)
        }
        .scrollIndicators(.hidden)
        .accessibilityIdentifier("identities-categories")
    }

    // MARK: Sections

    @ViewBuilder
    private func section(_ title: LocalizedStringKey, _ members: [Identity]) -> some View {
        if !members.isEmpty {
            Section {
                ForEach(members) { identity in
                    row(identity)
                }
            } header: {
                sectionHeader(title, count: members.count)
            }
        }
    }

    /// Kept folds away: what is left to decide comes first.
    @ViewBuilder
    private func keptSection(_ members: [Identity]) -> some View {
        if !members.isEmpty {
            Section {
                if showsKept || !query.isEmpty {
                    ForEach(members) { identity in
                        row(identity)
                    }
                }
            } header: {
                Button {
                    showsKept.toggle()
                } label: {
                    HStack {
                        sectionHeader("Kept", count: members.count)
                        Image(systemName: showsKept || !query.isEmpty ? "chevron.down" : "chevron.right")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityHint(Text(showsKept ? "Hides the kept names." : "Shows the kept names."))
            }
        }
    }

    private func sectionHeader(_ title: LocalizedStringKey, count: Int?) -> some View {
        HStack(spacing: Space.xxs) {
            Text(title)
            if let count {
                Text("\(count)")
                    .foregroundStyle(.tertiary)
            }
        }
        .font(Typography.caption.weight(.semibold))
        .foregroundStyle(.secondary)
        .textCase(nil)
        .accessibilityAddTraits(.isHeader)
    }

    // MARK: Rows

    private func row(_ identity: Identity) -> some View {
        let selected = selection.contains(identity.id)
        return HStack(spacing: Space.s) {
            if selecting {
                Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                    .font(.title3)
                    .foregroundStyle(selected ? Palette.accent : Color.secondary)
                    .accessibilityHidden(true)
            }
            Image(systemName: identity.category.symbol)
                .foregroundStyle(identity.category.color)
                .frame(width: 24)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                // Plain word wrap: iOS 26 hyphenates a Text mid-word
                // ("example.-com").
                UnhyphenatedText(identity.primaryWording)
                HStack(spacing: Space.xs) {
                    UnhyphenatedText(identity.alias, style: .footnote, monospaced: true, color: .secondaryLabel)
                        .fixedSize()
                    if identity.mentionCount > 1 {
                        Text("\(identity.mentionCount)×")
                            .font(Typography.caption)
                            .foregroundStyle(Color(uiColor: .secondaryLabel))
                            .fixedSize()
                    }
                    // Its other spellings, in grey.
                    let others = identity.variants.filter { $0 != identity.primaryWording }
                    if !others.isEmpty {
                        Text(verbatim: "· " + others.joined(separator: ", "))
                            .font(Typography.caption)
                            .foregroundStyle(Color(uiColor: .secondaryLabel))
                            .lineLimit(1)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if !selecting {
                Button {
                    model.toggle(identity)
                } label: {
                    StateButtonLabel(state: identity.state)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text(ReviewDetails.stateName(identity.state)))
                .accessibilityHint(Text(identity.state == .replaced ? "Keeps every mention." : "Replaces every mention."))
                .accessibilityIdentifier("identity-state")
            }
        }
        .padding(.vertical, Space.xxs)
        .contentShape(Rectangle())
        .onTapGesture {
            if selecting {
                if selected { selection.remove(identity.id) } else { selection.insert(identity.id) }
                Haptics.selection()
            } else {
                onOpen(identity.id)
            }
        }
        .listRowBackground(selected ? Palette.accent.opacity(0.08) : Color.clear)
        .listRowInsets(EdgeInsets(top: Space.xxs, leading: Space.m, bottom: Space.xxs, trailing: Space.xs))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(Self.spoken(identity)))
        .accessibilityAddTraits(selecting ? (selected ? [.isSelected, .isButton] : .isButton) : .isButton)
        .accessibilityHint(Text(selecting ? "Selects it." : "Opens it in the review bar."))
        .accessibilityAction(named: Text(identity.state == .replaced ? "Keep all" : "Replace all")) { model.toggle(identity) }
        .accessibilityAction(named: Text("Review this identity")) { onFocus(.identity(identity.id)) }
        .accessibilityAction(named: Text("Same as…")) { onPickSameAs(identity.id) }
        .accessibilityIdentifier("identity-row")
        // Right: Replace. Left: Keep, and Same as… (ADR 0098).
        .swipeActions(edge: .leading) {
            Button("Replace") { model.decide(identities: [identity.id], .replace) }
                .tint(Palette.accent)
        }
        .swipeActions(edge: .trailing) {
            Button("Keep") { model.decide(identities: [identity.id], .keep) }
                .tint(.gray)
            Button("Same as…") { onPickSameAs(identity.id) }
                .tint(.indigo)
        }
        .contextMenu {
            Button {
                onFocus(.identity(identity.id))
            } label: {
                Label("Review this identity", systemImage: "rectangle.stack")
            }
            Button {
                model.decide(identities: [identity.id], .replace)
            } label: {
                Label("Replace all", systemImage: "eye.slash")
            }
            Button {
                model.decide(identities: [identity.id], .keep)
            } label: {
                Label("Keep all", systemImage: "eye")
            }
            Button {
                onPickSameAs(identity.id)
            } label: {
                Label("Same as…", systemImage: "link")
            }
        }
        // Drag one row onto another: the same name (ADR 0098, iPad first).
        .draggable(identity.id.rawValue)
        .dropDestination(for: String.self) { items, _ in
            guard let dropped = items.first, dropped != identity.id.rawValue,
                  identities.contains(where: { $0.id.rawValue == dropped }) else { return false }
            model.link(IdentityID(rawValue: dropped), into: identity.id)
            return true
        }
    }

    // MARK: Select

    /// Replace, Keep and Same as for the ticked names.
    private var selectionBar: some View {
        let ids = identities.map(\.id).filter { selection.contains($0) }
        return HStack(spacing: Space.s) {
            Button {
                model.decide(identities: ids, .replace)
                selection = []
            } label: {
                Text("Replace").frame(maxWidth: .infinity, minHeight: Size.barButton)
            }
            .buttonStyle(.borderedProminent)
            Button {
                model.decide(identities: ids, .keep)
                selection = []
            } label: {
                Text("Keep").frame(maxWidth: .infinity, minHeight: Size.barButton)
            }
            .buttonStyle(.bordered)
            Button {
                model.merge(ids)
                selection = []
            } label: {
                Text("Same as").frame(maxWidth: .infinity, minHeight: Size.barButton)
            }
            .buttonStyle(.bordered)
            .disabled(ids.count < 2)
            .accessibilityHint(Text("Makes the selected names one name."))
        }
        .disabled(ids.isEmpty)
        .lineLimit(1)
        .minimumScaleFactor(0.8)
        .padding(.horizontal, Space.m)
        .padding(.vertical, Space.s)
        .background(.bar)
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("identities-selection-bar")
    }

    /// "Maya Bennett, also Ms Bennett, Person, PERSON_1, 3 mentions, proposed".
    static func spoken(_ identity: Identity) -> String {
        let state = switch identity.state {
        case .proposed: String(localized: "proposed")
        case .replaced: String(localized: "replaced")
        case .kept: String(localized: "kept")
        }
        let others = identity.variants.filter { $0 != identity.primaryWording }
        var parts = [identity.primaryWording]
        if !others.isEmpty { parts.append(String(localized: "also \(others.joined(separator: ", "))")) }
        parts += [identity.category.label, identity.alias, String(localized: "\(identity.mentionCount) mentions"), state]
        return parts.joined(separator: ", ")
    }
}

/// "Looks like: Maya Bennett · Ms Bennett", with Same as and ×.
private struct HintCard: View {
    let from: Identity
    let into: Identity
    let onSameAs: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: Space.s) {
            Image(systemName: "sparkles")
                .foregroundStyle(Palette.accent)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: "\(into.primaryWording) · \(from.primaryWording)")
                    .font(Typography.body)
                    .lineLimit(2)
                Text(verbatim: "\(into.alias) · \(from.alias)")
                    .font(Typography.alias)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            Button(action: onSameAs) {
                Chip("Same as", systemImage: "link", tint: Palette.accent)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("identities-hint-same-as")
            Button(action: onDismiss) {
                Image(systemName: "xmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: Size.target, height: Size.target)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("Not the same"))
        }
        .padding(.leading, Space.s)
        .card(padding: Space.xxs)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text("Looks like the same: \(into.primaryWording) and \(from.primaryWording)"))
    }
}

/// How much is decided, as a small ring.
struct ProgressRing: View {
    let fraction: Double

    var body: some View {
        ZStack {
            Circle().stroke(Palette.chipFill, lineWidth: 4)
            Circle()
                .trim(from: 0, to: max(0, min(1, fraction)))
                .stroke(Palette.accent, style: StrokeStyle(lineWidth: 4, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: 28, height: 28)
        .accessibilityHidden(true)
    }
}
