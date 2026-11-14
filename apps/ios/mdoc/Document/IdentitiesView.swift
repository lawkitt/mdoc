import MdocFFI
import SwiftUI

/// Identities (2026-11-06 grill, round 2; ADR 0115; docs/mobile-ui.md,
/// Identities): the document's names, a drawer from the right edge on iPhone
/// and the standing right column on iPad. At the top, pinned, the title with
/// how much is decided and Select, then search and a category filter; then
/// Looks like cards and the names by what is left: To review, Replaced,
/// Kept; Replace N floats at the bottom. Each row's round button replaces or
/// keeps that name; a tap opens the review bar's full view on it; Select
/// acts on several at once (ADR 0114: no close button and no hint line; the
/// sliver, a swipe, 🪪 and VoiceOver's escape close the drawer).
struct IdentitiesView: View {
    @Bindable var model: DocumentViewModel
    /// The bar's full view on a name (and the drawer closed on iPhone).
    let onOpen: (IdentityID) -> Void
    /// Review this identity or a category (focus), from a row's menu.
    var onFocus: (ReviewFocus) -> Void = { _ in }
    /// Same as… for one identity: the shared picker, hints first.
    var onPickSameAs: (IdentityID) -> Void = { _ in }

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State var query = ""
    @FocusState private var searchFocused: Bool
    /// The filter's category; nil is All.
    @State private var category: Category?
    /// The Select demo shows (`IdentitiesView+Select`), and its beats.
    @State var demoShows = false
    @State var demoTask: Task<Void, Never>?
    @State var selecting = false
    @State var selection: Set<IdentityID> = []
    @State var showsKept = false
    /// Names just decided, at the state whose section they stay in while
    /// their marks turn (`IdentitiesView+Gestures`).
    @State var held: [IdentityID: MentionState] = [:]
    /// A row lifted by its ≡ handle (`IdentitiesView+DragMerge`).
    @State var drag: IdentityDragState?
    /// The rows' frames, which the drag finds its target in.
    @State var dragTracking = IdentityDragTracking()
    @Environment(\.accessibilityReduceMotion) var reduceMotion

    var identities: [Identity] { model.snapshot.identities }

    var shown: [Identity] {
        let needle = query.trimmingCharacters(in: .whitespaces)
        return identities.filter { identity in
            (filtered == nil || identity.category == filtered)
                && (needle.isEmpty || identity.alias.localizedStandardContains(needle)
                    || identity.variants.contains { $0.localizedStandardContains(needle) }
                    || identity.primaryWording.localizedStandardContains(needle))
        }
    }

    /// The filter's category while names of it are left; else All.
    private var filtered: Category? {
        categories.contains { $0.0 == category } ? category : nil
    }

    /// The filter shows with two categories or more, or one chosen.
    private var offersFilter: Bool { categories.count > 1 || filtered != nil }

    /// Replace N floats at the panel's bottom while names are left, outside
    /// Select, whose bar takes its place (ADR 0115).
    private var floatsReplaceAll: Bool { !selecting && model.identityCounts.toReview > 0 }

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
                section("To review", id: "proposed", shown.filter { placed($0) == .proposed })
                section("Replaced", id: "replaced", shown.filter { placed($0) == .replaced })
                keptSection(shown.filter { placed($0) == .kept })
            }
            .listStyle(.plain)
            .listSectionSpacing(Space.xs)
            .environment(\.defaultMinListHeaderHeight, 0)
            .scrollContentBackground(.hidden)
            // The header above is opaque and the section headers pin under
            // it: no blur of the rows at the list's top (ADR 0115).
            .scrollEdgeEffectHidden(true, for: .top)
            .scrollDismissesKeyboard(.immediately)
            .contentMargins(.bottom, selecting || floatsReplaceAll ? 72 : Space.m, for: .scrollContent)
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { dragTracking.list = $0 }
            // A scroll moves the rows under the demo's ghost: it ends the demo.
            .onScrollPhaseChange { _, phase in
                if phase == .interacting { endDemo(dismissed: true) }
            }
            .overlay { liftedCopy }
        }
        .background(Palette.background)
        .overlay(alignment: .bottom) {
            if selecting {
                selectionBar
            } else if floatsReplaceAll {
                replaceAllButton
                    .transition(.opacity.combined(with: .move(edge: .bottom)))
            }
        }
        .animation(Motion.snappy, value: selecting)
        .animation(Motion.snappy, value: floatsReplaceAll)
        .animation(Motion.fade, value: demoShows)
        // Any tap ends the Select demo, and still does what it taps.
        .simultaneousGesture(TapGesture().onEnded { endDemo(dismissed: true) }, including: demoShows ? .all : .subviews)
        // The handles drag only in Select; leaving it ends a drag.
        .onChange(of: selecting, initial: true) { _, now in
            dragTracking.active = now
            if now {
                startDemoIfDue()
            } else {
                endDemo(dismissed: false)
                endDrag()
            }
        }
        .onDisappear {
            endDemo(dismissed: false)
            forgetDrag()
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("identities-panel")
    }

    // MARK: Header

    /// Two rows pinned above the list (ADR 0115): the title with "8 of 12
    /// decided" under it and Select; then search and the category filter.
    /// In Select, Cancel, "3 selected" and Select All, as in Mail, take the
    /// first row's place.
    private var header: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            if selecting {
                selectHeader
            } else if dynamicTypeSize.isAccessibilitySize {
                // The largest sizes: the title and summary alone on the
                // first row, Select beside the filter under the search.
                titleAndSummary
            } else {
                HStack(alignment: .center, spacing: Space.s) {
                    titleAndSummary
                    if !identities.isEmpty {
                        selectButton
                    }
                }
            }
            if demoShows {
                SelectDemoCaption()
                    .transition(.opacity)
            }
            if !identities.isEmpty {
                if dynamicTypeSize.isAccessibilitySize {
                    searchField
                    HStack(spacing: Space.xs) {
                        if offersFilter {
                            filterMenu
                        }
                        Spacer(minLength: 0)
                        if !selecting {
                            selectButton
                        }
                    }
                } else {
                    HStack(spacing: Space.xs) {
                        searchField
                        if offersFilter {
                            filterMenu
                        }
                    }
                }
            }
        }
        .padding(.horizontal, Space.m)
        .padding(.top, Space.s)
        .padding(.bottom, Space.xs)
        .background(Palette.background)
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
    }

    /// "Identities" with the summary under it, each on one line.
    private var titleAndSummary: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Identities")
                .font(Typography.title)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
                .accessibilityAddTraits(.isHeader)
            if let summary = summary(model.identityCounts) {
                Text(summary)
                    .font(Typography.caption)
                    .foregroundStyle(.secondary)
                    // One line: the words shrink a little, then truncate.
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                    .accessibilityIdentifier("identities-summary")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// "8 of 12 decided"; once none is left, "12 names · all decided ✓".
    private func summary(_ counts: (total: Int, toReview: Int)) -> String? {
        guard counts.total > 0 else { return nil }
        guard counts.toReview == 0 else {
            return String(localized: "\(counts.total - counts.toReview) of \(counts.total) decided")
        }
        let names = counts.total == 1 ? String(localized: "1 name") : String(localized: "\(counts.total) names")
        return names + " · " + String(localized: "all decided ✓")
    }

    /// The search field, always shown: names, wordings and aliases.
    private var searchField: some View {
        HStack(spacing: Space.xs) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            TextField("Search names", text: $query)
                .focused($searchFocused)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.search)
                .accessibilityIdentifier("identities-search")
            if !query.isEmpty {
                Button {
                    query = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                        .frame(width: 28, height: Size.target)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Clear search"))
                .accessibilityIdentifier("identities-search-clear")
            }
        }
        .font(Typography.body)
        .padding(.leading, Space.s)
        .padding(.trailing, query.isEmpty ? Space.s : Space.xxs)
        .frame(minHeight: Size.target)
        .background(Palette.chipFill, in: Capsule())
        .frame(maxWidth: .infinity)
    }

    // MARK: Looks like
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

    /// One capsule in place of the chips (ADR 0115): its menu lists All and
    /// each category with its icon and count; its label, the filter now.
    private var filterMenu: some View {
        Menu {
            Picker(selection: $category) {
                Label("All \(identities.count)", systemImage: "person.2")
                    .tag(Category?.none)
                ForEach(categories, id: \.0) { item in
                    Label("\(item.0.label) \(item.1)", systemImage: item.0.symbol)
                        .tag(Category?.some(item.0))
                }
            } label: {
                Text("Show")
            }
            .pickerStyle(.inline)
        } label: {
            HStack(spacing: Space.xxs) {
                Image(systemName: filtered?.symbol ?? "line.3.horizontal.decrease")
                    .foregroundStyle(filtered?.color ?? .primary)
                Text(filtered?.label ?? String(localized: "All"))
                    .lineLimit(1)
                Image(systemName: "chevron.up.chevron.down")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            .font(Typography.chip)
            .foregroundStyle(.primary)
            .padding(.horizontal, Space.s)
            .frame(minHeight: Size.target)
            .background(Palette.chipFill, in: Capsule())
            .contentShape(Capsule())
        }
        .fixedSize()
        .accessibilityLabel(Text("Show"))
        .accessibilityValue(Text(filtered?.label ?? String(localized: "All")))
        .accessibilityHint(Text("Shows the names of one category."))
        .accessibilityIdentifier("identities-filter")
    }

    /// Replace N, a glass capsule over the list's bottom.
    private var replaceAllButton: some View {
        let left = model.identityCounts.toReview
        return Button {
            deciding(identities.filter { $0.state == .proposed }.map(\.id)) { model.replaceAllPending() }
        } label: {
            Label("Replace \(left)", systemImage: "eye.slash")
                .padding(.horizontal, Space.xs)
        }
        // A bulk shortcut past looking at each name: clear glass, not the
        // drawer's loudest thing (buttons grill Q2).
        .secondaryAction()
        .padding(.bottom, Space.s)
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
        .accessibilityLabel(Text("Replace all \(left) names left"))
        .accessibilityIdentifier("identities-replace-all")
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
                    sectionHeader("Kept", id: "kept", count: members.count,
                                  chevron: showsKept || !query.isEmpty ? "chevron.down" : "chevron.right")
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .listRowInsets(EdgeInsets())
                .accessibilityHint(Text(showsKept ? "Hides the kept names." : "Shows the kept names."))
            }
        }
    }

    /// A section's title and count on an opaque band the width of the list
    /// (ADR 0115: no row shows through a pinned header); `id` names its
    /// frame, which a drag keeps clear of (the header pins over the rows).
    private func sectionHeader(_ title: LocalizedStringKey, id: String, count: Int?, chevron: String? = nil) -> some View {
        HStack(spacing: Space.xxs) {
            // Label colours, not `.primary` and `.secondary`: a List header's
            // own style dims those a level.
            Text(title)
                .foregroundStyle(Color(uiColor: .label))
            if let count {
                Text("\(count)")
                    .foregroundStyle(Color(uiColor: .secondaryLabel))
            }
            if let chevron {
                Image(systemName: chevron)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Color(uiColor: .secondaryLabel))
                    .padding(.leading, Space.xxs)
                    .accessibilityHidden(true)
            }
            Spacer(minLength: 0)
        }
        .font(Typography.caption.weight(.semibold))
        .textCase(nil)
        .padding(.horizontal, Space.m)
        .padding(.vertical, Space.xs)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.background)
        .listRowInsets(EdgeInsets())
        .accessibilityElement(children: .combine)
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
