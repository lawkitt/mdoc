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

    @ViewBuilder
    private var stateBadge: some View {
        switch candidate.state {
        case .replaced:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(Palette.ready)
                .accessibilityLabel(Text("Replaced"))
        case .kept:
            Image(systemName: "eye")
                .foregroundStyle(.secondary)
                .accessibilityLabel(Text("Kept"))
        case .proposed:
            EmptyView()
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
                .disabled(choice.isCurrent)
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
        if targets.all.count > 8 {
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
            Picker("Scope", selection: $model.reviewScope) {
                Text("All \(count)").tag(Scope.allOccurrences)
                Text("This one").tag(Scope.justThisOne)
            }
        } label: {
            Chip(model.reviewScope == .allOccurrences ? Text("All \(count)") : Text("This one"), opensMenu: true)
        }
        .disabled(count < 2)
        .accessibilityLabel(Text(model.reviewScope == .allOccurrences ? "Scope, all \(count) mentions" : "Scope, this one"))
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

    private func hintLine(_ hint: MergeHint) -> some View {
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

/// Swipe up: the identity's mentions with their context, and Keep all or
/// Replace all for it (ADR 0098's expanded view).
struct ReviewDetails: View {
    @Bindable var model: DocumentViewModel
    let candidate: Candidate

    var body: some View {
        let mentions = model.snapshot.mentions(of: candidate.identityID).sorted { $0.range.start < $1.range.start }
        // As tall as its content, scrolling only past the cap.
        ViewThatFits(in: .vertical) {
            content(mentions)
            ScrollView { content(mentions) }
                .scrollBounceBehavior(.basedOnSize)
        }
        .accessibilityIdentifier("review-details")
    }

    private func content(_ mentions: [Candidate]) -> some View {
            VStack(alignment: .leading, spacing: Space.xs) {
                Text("Mentions (\(mentions.count))")
                    .font(Typography.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                ForEach(mentions) { mention in
                    Button {
                        model.select(mention.id)
                    } label: {
                        HStack(alignment: .firstTextBaseline, spacing: Space.xs) {
                            Image(systemName: Self.symbol(mention.state))
                                .foregroundStyle(mention.state == .replaced ? Palette.ready : .secondary)
                                .accessibilityLabel(Text(Self.stateName(mention.state)))
                            Text(model.context(of: mention))
                                .font(Typography.caption)
                                .foregroundStyle(.primary)
                                .lineLimit(2)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .padding(.vertical, Space.xxs)
                        .background(mention.id == candidate.id ? Palette.accent.opacity(0.08) : .clear)
                    }
                    .buttonStyle(.plain)
                }
                if mentions.count > 1 {
                    HStack(spacing: Space.s) {
                        Button("Keep all \(mentions.count)") {
                            model.decideIdentity(.keep)
                        }
                        .buttonStyle(.bordered)
                        Button("Replace all \(mentions.count)") {
                            model.decideIdentity(.replace)
                        }
                        .buttonStyle(.bordered)
                    }
                    .padding(.top, Space.xs)
                }
            }
            .padding(.vertical, Space.xs)
    }

    static func symbol(_ state: MentionState) -> String {
        switch state {
        case .proposed: "circle.dashed"
        case .replaced: "checkmark.circle.fill"
        case .kept: "eye"
        }
    }

    static func stateName(_ state: MentionState) -> String {
        switch state {
        case .proposed: String(localized: "Proposed")
        case .replaced: String(localized: "Replaced")
        case .kept: String(localized: "Kept")
        }
    }
}
