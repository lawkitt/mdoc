import MdocFFI
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
    @State var selecting = false
    @State var selection: Set<IdentityID> = []
    @State private var showsKept = false
    /// Names just decided, at the state whose section they stay in while
    /// their marks turn (`IdentitiesView+Gestures`).
    @State var held: [IdentityID: MentionState] = [:]
    /// A row lifted by its ≡ handle (`IdentitiesView+DragMerge`).
    @State var drag: IdentityDragState?
    /// The rows' frames, which the drag finds its target in.
    @State var dragTracking = IdentityDragTracking()
    @Environment(\.accessibilityReduceMotion) var reduceMotion

    var identities: [Identity] { model.snapshot.identities }

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
                section("To review", id: "proposed", shown.filter { placed($0) == .proposed })
                section("Replaced", id: "replaced", shown.filter { placed($0) == .replaced })
                keptSection(shown.filter { placed($0) == .kept })
            }
            .listStyle(.plain)
            .listSectionSpacing(Space.xs)
            .scrollContentBackground(.hidden)
            .contentMargins(.bottom, selecting ? 72 : Space.m, for: .scrollContent)
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { dragTracking.list = $0 }
            .overlay { liftedCopy }
        }
        .background(Palette.background)
        .overlay(alignment: .bottom) {
            if selecting { selectionBar }
        }
        .animation(Motion.snappy, value: selecting)
        .animation(Motion.snappy, value: searching)
        // The handles drag only in Select; leaving it ends a drag.
        .onChange(of: selecting, initial: true) { _, now in
            dragTracking.active = now
            if !now { endDrag() }
        }
        .onDisappear { forgetDrag() }
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
                    .textAction()
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
                    deciding(identities.filter { $0.state == .proposed }.map(\.id)) { model.replaceAllPending() }
                } label: {
                    Label("Replace \(counts.toReview)", systemImage: "eye.slash")
                        .frame(maxWidth: .infinity)
                }
                // A bulk shortcut past looking at each name: clear glass,
                // not the drawer's loudest thing (buttons grill Q2).
                .secondaryAction()
                .accessibilityLabel(Text("Replace all \(counts.toReview) names left"))
                .accessibilityIdentifier("identities-replace-all")
            }
            if model.showsDragMergeHint && !selecting {
                ReviewHintLine(hint: .dragMerge) { model.learn(.dragMerge) }
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
                sectionHeader("Looks like", id: "hints", count: nil)
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
                    Chip(Text("All"), filled: category == nil)
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
    private func section(_ title: LocalizedStringKey, id: String, _ members: [Identity]) -> some View {
        if !members.isEmpty {
            Section {
                ForEach(members) { identity in
                    row(identity)
                }
            } header: {
                sectionHeader(title, id: id, count: members.count)
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
                        sectionHeader("Kept", id: "kept", count: members.count)
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

    /// A section's title and count; `id` names its frame, which a drag
    /// keeps clear of (the header pins over the rows).
    private func sectionHeader(_ title: LocalizedStringKey, id: String, count: Int?) -> some View {
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
        .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { dragTracking.headers[id] = $0 }
        .onDisappear { dragTracking.headers[id] = nil }
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
                Text("Replace").frame(maxWidth: .infinity)
            }
            .primaryAction()
            Button {
                model.decide(identities: ids, .keep)
                selection = []
            } label: {
                Text("Keep").frame(maxWidth: .infinity)
            }
            .secondaryAction()
            Button {
                model.merge(ids)
                selection = []
            } label: {
                Text("Same as").frame(maxWidth: .infinity)
            }
            .secondaryAction()
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
        // A drag finds no name under the bar, and scrolls at its top.
        .onGeometryChange(for: CGFloat.self) { $0.frame(in: .global).minY } action: { dragTracking.barTop = $0 }
        .onDisappear { dragTracking.barTop = nil }
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
