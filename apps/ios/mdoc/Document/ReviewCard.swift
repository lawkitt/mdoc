import MdocFFI
import SwiftUI

/// The swipeable card under review (ADR 0098): the category icon, the name
/// (tap to scroll back to it) and its alias, with a mentions chip when the
/// identity has more than one; then the category, alias and scope chips;
/// and a merge hint while the core sees one.
struct ReviewCard: View {
    @Bindable var model: DocumentViewModel
    let candidate: Candidate

    @State private var renaming = false
    @State private var draft = ""
    @State private var renameProblem: DocumentViewModel.RenameOutcome?
    @State private var showsSameAsList = false
    @FocusState private var aliasFocused: Bool

    private var identity: Identity? { model.snapshot.identities.first { $0.id == candidate.identityID } }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            header
            if renaming {
                renameField
            } else {
                chips
            }
            if let hint = model.mergeHint(for: candidate), !renaming {
                hintLine(hint)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(padding: Space.s)
        .sheet(isPresented: $showsSameAsList) {
            SameAsPicker(
                source: candidate.original, targets: model.sameAsTargets(for: candidate),
                categoryLabel: candidate.category.label) { model.sameAs($0) }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("review-card")
    }

    // MARK: Line 1

    private var header: some View {
        HStack(spacing: Space.xs) {
            Image(systemName: candidate.category.symbol)
                .foregroundStyle(candidate.category.color)
                .accessibilityHidden(true)
            Button {
                model.revealReviewed()
            } label: {
                Text(candidate.original)
                    .font(Typography.title)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .buttonStyle(.plain)
            .accessibilityHint(Text("Scrolls the text to it."))
            Spacer(minLength: 0)
            stateBadge
            let mentions = model.mentionPosition(of: candidate)
            if mentions.count > 1 {
                Button {
                    model.nextMention()
                } label: {
                    Chip(Text("\(mentions.index) of \(mentions.count)"), systemImage: "arrow.down")
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Mention \(mentions.index) of \(mentions.count)"))
                .accessibilityHint(Text("Scrolls to this name's next mention."))
                .accessibilityIdentifier("review-mentions")
            }
        }
    }

    /// Replaced or kept, in the mention rows' round marks; nothing while
    /// proposed.
    @ViewBuilder
    private var stateBadge: some View {
        if candidate.state != .proposed {
            StateMark(state: candidate.state)
                .accessibilityLabel(Text(ReviewDetails.stateName(candidate.state)))
        }
    }

    // MARK: Line 2

    private var chips: some View {
        ScrollView(.horizontal) {
            HStack(spacing: Space.xs) {
                categoryMenu
                aliasMenu
                scopeMenu
            }
        }
        .scrollIndicators(.hidden)
        .scrollClipDisabled()
    }

    private var categoryMenu: some View {
        Menu {
            ForEach(model.categoryChoices(for: candidate), id: \.category) { choice in
                Button {
                    model.setCategory(choice.category)
                } label: {
                    Label {
                        // "Organization → ORG_3", the outcome before the tap.
                        if let alias = choice.alias {
                            Text(verbatim: "\(choice.category.label) → \(alias)")
                        } else {
                            Text(verbatim: choice.category.label)
                        }
                    } icon: {
                        Image(systemName: choice.category.symbol)
                    }
                }
                .disabled(choice.current)
            }
            // Belongs to… for contacts and identifiers, or once an owner is
            // set (ADR 0032).
            if candidate.category.hasOwner || identity?.owner != nil {
                let owners = model.snapshot.identities.filter { ($0.category == .person || $0.category == .organization) && $0.id != candidate.identityID }
                Menu {
                    Button {
                        model.setOwner(nil)
                    } label: {
                        if identity?.owner == nil { Label("Nobody", systemImage: "checkmark") } else { Text("Nobody") }
                    }
                    ForEach(owners) { owner in
                        Button {
                            model.setOwner(owner.id)
                        } label: {
                            if identity?.owner == owner.id {
                                Label("\(owner.primaryWording) · \(owner.alias)", systemImage: "checkmark")
                            } else {
                                Text(verbatim: "\(owner.primaryWording) · \(owner.alias)")
                            }
                        }
                    }
                } label: {
                    Label(ownerTitle, systemImage: "person.crop.circle.badge.questionmark")
                }
            }
        } label: {
            Chip(candidate.category.label, systemImage: candidate.category.symbol, tint: candidate.category.color, opensMenu: true)
        }
        .accessibilityLabel(Text("Category, \(candidate.category.label)"))
        .accessibilityIdentifier("review-category")
    }

    private var ownerTitle: String {
        if let owner = identity?.owner, let name = model.snapshot.identities.first(where: { $0.id == owner }) {
            return String(localized: "Belongs to \(name.alias)")
        }
        return String(localized: "Belongs to…")
    }

    private var aliasMenu: some View {
        Menu {
            Button {
                draft = candidate.alias
                renameProblem = nil
                renaming = true
                aliasFocused = true
            } label: {
                Label("Rename…", systemImage: "pencil")
            }
            sameAsMenu
            Button {
                model.newAlias()
            } label: {
                Label("New alias", systemImage: "plus.square.on.square")
            }
            .disabled((identity?.mentionCount ?? 0) < 2)
        } label: {
            Chip(candidate.alias, opensMenu: true, monospaced: true)
        }
        .accessibilityLabel(Text("Alias, \(candidate.alias)"))
        .accessibilityIdentifier("review-alias")
    }

    /// Hinted identities first, then the same category, then the others; a
    /// long list opens a searchable sheet instead (ADR 0098).
    @ViewBuilder
    private var sameAsMenu: some View {
        let targets = model.sameAsTargets(for: candidate)
        if targets.all.count > DocumentViewModel.sameAsListLimit {
            Button {
                showsSameAsList = true
            } label: {
                Label("Same as…", systemImage: "link")
            }
        } else if !targets.all.isEmpty {
            Menu {
                if !targets.hinted.isEmpty {
                    Section("Looks like") {
                        ForEach(targets.hinted) { target in sameAsButton(target) }
                    }
                }
                Section {
                    ForEach(targets.sameCategory) { target in sameAsButton(target) }
                }
                if !targets.otherCategories.isEmpty {
                    Menu("Other categories") {
                        ForEach(targets.otherCategories) { target in sameAsButton(target) }
                    }
                }
            } label: {
                Label("Same as…", systemImage: "link")
            }
        }
    }

    private func sameAsButton(_ target: Identity) -> some View {
        Button {
            model.sameAs(target.id)
        } label: {
            Text(verbatim: "\(target.primaryWording) · \(target.alias) · \(target.mentionCount)")
        }
    }

    private var scopeMenu: some View {
        let count = identity?.mentionCount ?? 1
        return Menu {
            Picker("Scope", selection: Binding(get: { model.reviewScope }, set: { model.setScope($0) })) {
                Text("All \(count)").tag(Scope.all)
                Text("This one").tag(Scope.this)
            }
        } label: {
            Chip(model.reviewScope == .all ? Text("All \(count)") : Text("This one"), opensMenu: true)
        }
        .disabled(count < 2)
        .accessibilityLabel(Text(model.reviewScope == .all ? "Scope, all \(count) mentions" : "Scope, this one"))
        .accessibilityIdentifier("review-scope")
    }

    // MARK: Rename

    private var renameField: some View {
        VStack(alignment: .leading, spacing: Space.xxs) {
            HStack(spacing: Space.xs) {
                TextField("Alias", text: $draft)
                    .font(Typography.alias)
                    .textInputAutocapitalization(.characters)
                    .autocorrectionDisabled()
                    .focused($aliasFocused)
                    .submitLabel(.done)
                    .onSubmit(commitRename)
                    .padding(.horizontal, Space.s)
                    .frame(minHeight: Size.chip)
                    .background(Palette.chipFill, in: Capsule())
                    .accessibilityIdentifier("review-rename-field")
                Button("Cancel") { renaming = false }
                    .frame(minHeight: Size.target)
                Button("Rename", action: commitRename)
                    .fontWeight(.semibold)
                    .frame(minHeight: Size.target)
                    .disabled(draft.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            switch renameProblem {
            case .taken(let other):
                HStack(spacing: Space.xs) {
                    Text("\(other.alias) is \(other.primaryWording)")
                        .font(Typography.caption)
                        .foregroundStyle(.secondary)
                    Button {
                        renaming = false
                        model.sameAs(other.id)
                    } label: {
                        Chip("Same as", systemImage: "link", tint: Palette.accent)
                    }
                    .buttonStyle(.plain)
                }
            case .invalid(let message):
                Text(message)
                    .font(Typography.caption)
                    .foregroundStyle(.red)
            case .done, nil:
                EmptyView()
            }
        }
    }

    private func commitRename() {
        let outcome = model.rename(to: draft.trimmingCharacters(in: .whitespaces))
        if case .done = outcome {
            renaming = false
            renameProblem = nil
        } else {
            renameProblem = outcome
        }
    }

    // MARK: Merge hint

    private func hintLine(_ hint: LinkHint) -> some View {
        HStack(spacing: Space.xs) {
            Image(systemName: "sparkles")
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            Text("Looks like \(hint.wording)")
                .font(Typography.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Spacer(minLength: 0)
            Button {
                model.sameAs(model.hintTarget(hint, for: candidate))
            } label: {
                Chip("Same as", systemImage: "link", tint: Palette.accent)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("review-hint-same-as")
            Button {
                model.dismissHint(hint)
            } label: {
                Image(systemName: "xmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: Size.target, height: Size.chip)
            }
            .accessibilityLabel(Text("Dismiss hint"))
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("review-hint")
    }
}

/// Same as… with a long list of identities: a searchable list, hints
/// first (ADR 0098). The card, the long press and Identities share it.
struct SameAsPicker: View {
    let source: String
    let targets: DocumentViewModel.SameAsTargets
    let categoryLabel: String
    let onPick: (IdentityID) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    var body: some View {
        NavigationStack {
            List {
                section(String(localized: "Looks like"), targets.hinted)
                section(categoryLabel, targets.sameCategory)
                section(String(localized: "Other categories"), targets.otherCategories)
            }
            .searchable(text: $query)
            .navigationTitle("Same as…")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .principal) {
                    VStack(spacing: 0) {
                        Text("Same as…").font(.headline)
                        Text(source).font(Typography.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .accessibilityIdentifier("same-as-picker")
    }

    @ViewBuilder
    private func section(_ title: String, _ identities: [Identity]) -> some View {
        let shown = identities.filter { identity in
            query.isEmpty || identity.alias.localizedStandardContains(query)
                || identity.variants.contains { $0.localizedStandardContains(query) }
        }
        if !shown.isEmpty {
            Section(title) {
                ForEach(shown) { target in
                    Button {
                        dismiss()
                        onPick(target.id)
                    } label: {
                        LabeledContent {
                            Text("\(target.mentionCount)")
                        } label: {
                            VStack(alignment: .leading) {
                                Text(target.primaryWording)
                                Text(target.alias).font(Typography.alias).foregroundStyle(.secondary)
                            }
                        }
                    }
                    .foregroundStyle(.primary)
                }
            }
        }
    }
}

/// Pulled up: one row per mention of the identity under review, in
/// document order. Each shows the words around the name, the name in its
/// original wording, and the heading above it; a tap scrolls the text there,
/// and the state pill changes that one mention (docs/mobile-ui.md).
struct ReviewDetails: View {
    @Bindable var model: DocumentViewModel
    let candidate: Candidate

    var body: some View {
        let mentions = model.sortedMentions(of: candidate.identityID)
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(mentions.enumerated()), id: \.element.id) { offset, mention in
                MentionRow(
                    snippet: model.snippet(of: mention), state: mention.state,
                    isCurrent: mention.id == candidate.id, tint: candidate.category.color,
                    onOpen: { model.openMention(mention.id) },
                    onToggle: {
                        model.decideMention(mention.id, mention.state == .replaced ? .keep : .replace)
                    })
                if offset < mentions.count - 1 {
                    Divider().padding(.leading, Space.s)
                }
            }
        }
        .padding(.vertical, Space.xxs)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("review-details")
    }

    static func stateName(_ state: MentionState) -> String {
        switch state {
        case .proposed: String(localized: "Proposed")
        case .replaced: String(localized: "Replaced")
        case .kept: String(localized: "Kept")
        }
    }

    static func symbol(_ state: MentionState) -> String {
        switch state {
        case .proposed: "circle.dashed"
        case .replaced: "eye.slash"
        case .kept: "eye"
        }
    }
}

/// One mention: its heading and words, the name highlighted as in the
/// text, and a pill with its state.
struct MentionRow: View {
    let snippet: MentionSnippet
    let state: MentionState
    let isCurrent: Bool
    let tint: Color
    let onOpen: () -> Void
    let onToggle: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: Space.s) {
            Button(action: onOpen) {
                VStack(alignment: .leading, spacing: 2) {
                    if let heading = snippet.heading {
                        Text(heading)
                            .font(Typography.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    Text(words)
                        .font(Typography.caption)
                        .foregroundStyle(.primary)
                        .lineLimit(3)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text(spoken))
            .accessibilityHint(Text("Scrolls the text to it."))
            Button(action: onToggle) {
                StateButtonLabel(state: state)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text(ReviewDetails.stateName(state)))
            .accessibilityHint(Text(state == .replaced ? "Keeps this one." : "Replaces this one."))
            .accessibilityIdentifier("mention-state")
        }
        .padding(.horizontal, Space.s)
        .padding(.vertical, Space.xxs)
        .background(
            isCurrent ? Palette.accent.opacity(0.08) : .clear,
            in: RoundedRectangle(cornerRadius: Radius.control, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityAddTraits(isCurrent ? .isSelected : [])
        .accessibilityIdentifier("mention-row")
    }

    /// The words, with the name bold on its category's highlight.
    private var words: AttributedString {
        var name = AttributedString(snippet.name)
        name.inlinePresentationIntent = .stronglyEmphasized
        name.backgroundColor = tint.opacity(0.18)
        return AttributedString(snippet.before) + name + AttributedString(snippet.after)
    }

    private var spoken: String {
        [snippet.heading, snippet.before + snippet.name + snippet.after]
            .compactMap { $0 }
            .joined(separator: ", ")
            .replacingOccurrences(of: "…", with: "")
    }
}

/// A mention's or an identity's state as a round button, like Reminders'
/// checkbox: a dashed ring while proposed, a filled accent disc once
/// replaced, an eye once kept. The words are for VoiceOver.
struct StateButtonLabel: View {
    let state: MentionState

    var body: some View {
        StateMark(state: state)
            // The whole 44-point square is the target.
            .frame(width: Size.target, height: Size.target)
            .contentShape(Rectangle())
    }
}

/// The round mark itself, 26 points.
struct StateMark: View {
    let state: MentionState

    var body: some View {
        ZStack {
            switch state {
            case .proposed:
                Circle()
                    .strokeBorder(Color.secondary, style: StrokeStyle(lineWidth: 1.5, dash: [3, 3]))
            case .replaced:
                Circle().fill(Palette.accent)
                Image(systemName: "eye.slash.fill")
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(.white)
            case .kept:
                Circle().fill(Palette.chipFill)
                Image(systemName: "eye")
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(.primary)
            }
        }
        .frame(width: 26, height: 26)
    }
}
