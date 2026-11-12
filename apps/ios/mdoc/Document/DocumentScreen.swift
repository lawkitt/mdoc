import MdocFFI
import SwiftUI
import UIKit

/// One document: Text and Original switch on iPhone, from the panes' row
/// under the bar (ADR 0111), and sit side by side on iPad, with the
/// Identities list beside the text there. In a narrow iPad
/// window (portrait) Identities take the Original's place instead of a third
/// column (ADR 0087). Where Identities show comes from the window's
/// `DocumentLayout`; whether they are wanted, from the model.
struct DocumentScreen: View {
    @Bindable var model: DocumentViewModel
    let setup: SetupViewModel
    @Environment(\.documentLayout) private var layout
    @Environment(\.undoManager) private var undoManager
    @Environment(\.showSidebar) private var showSidebar
    @Environment(\.appModel) private var app
    @Environment(\.preferences) private var preferences

    private var isRegular: Bool { layout.isRegular }
    @State private var screenHeight: CGFloat = 800
    @State private var screenWidth: CGFloat = 0
    /// The long press's Same as… with a long list: the searchable picker.
    @State private var sameAsCandidate: CandidateID?
    /// Identities' Same as…: the identity it starts from.
    @State private var sameAsIdentity: IdentityID?
    /// This screen, for the window's undo: a size-class change builds a new
    /// one for the same document (#672).
    @State private var screenID = UUID()

    private var hasRightPane: Bool { model.hasRightPane }

    /// Identities show in the right column rather than as an inspector.
    private var identitiesInPane: Bool { model.identitiesPlacement == .column }

    var body: some View {
        content
            // Pinned: the text scrolls under the row, never with it.
            .safeAreaBar(edge: .top, spacing: 0) { panesRow }
            .navigationTitle(model.snapshot.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbar }
            // Above the bottom bar, never over its buttons.
            .toast($model.toast)
            .safeAreaInset(edge: .bottom) { bottomBar }
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { screenHeight = $0 }
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { screenWidth = $0 }
            .inspector(isPresented: inspectorBinding) {
                identities
                    .inspectorColumnWidth(min: 260, ideal: 320, max: 420)
                    .reviewSheetDetents()
            }
            .sheet(isPresented: $model.showsFind) {
                FindSheet(model: model)
            }
            .sheet(item: $model.linkDraft) { draft in
                LinkSheet(draft: draft)
            }
            .sheet(item: Binding(get: { sameAsCandidate.map(PickerSource.candidate) ?? sameAsIdentity.map(PickerSource.identity) },
                                 set: { if $0 == nil { sameAsCandidate = nil; sameAsIdentity = nil } })) { source in
                sameAsPicker(source)
            }
            .sheet(isPresented: $model.showsReplySheet) {
                ReplySheet(caseName: model.snapshot.caseName) { reply in
                    model.restore(reply: reply)
                }
            }
            .sheet(item: $model.share, onDismiss: nil) { items in
                ActivityView(items: items.items)
            }
            .alert(
                "Something went wrong",
                isPresented: Binding(get: { model.errorMessage != nil }, set: { if !$0 { model.errorMessage = nil } }),
                presenting: model.errorMessage
            ) { _ in
                Button("OK") { model.errorMessage = nil }
            } message: { message in
                Text(message)
            }
            .onAppear {
                // The window's undo holds this document's steps only (#672).
                model.attachSystemUndo(undoManager, for: screenID)
                model.refresh()
            }
            .onDisappear {
                // Leaving the document ends edit mode; its save follows (ADR 0109).
                model.doneEditing()
                model.detachSystemUndo(for: screenID)
            }
            // Where Identities show follows the window (ADR 0104).
            .onChange(of: layout, initial: true) { _, layout in model.windowChanged(to: layout) }
    }

    private var identities: some View {
        IdentitiesPanel(model: model)
    }

    /// The inspector stays closed while Identities show in the right column.
    /// On iPhone Identities is the right-edge drawer (`IdentitiesDrawer`), so
    /// the inspector, a sheet there, stays closed.
    private var inspectorBinding: Binding<Bool> {
        Binding(
            get: { model.identitiesPlacement == .inspector && model.wantsIdentities },
            set: { model.showIdentities($0) })
    }


    // MARK: Panes

    @ViewBuilder
    private var content: some View {
        if isRegular && hasRightPane {
            HStack(spacing: 0) {
                textPane
                Divider()
                if identitiesInPane && model.wantsIdentities {
                    identities
                } else if model.pane == .reply, let reply = model.snapshot.reply {
                    ReplyPane(reply: reply, onClear: model.clearReply)
                } else {
                    OriginalView(original: model.snapshot.original)
                }
            }
        } else {
            // The text stays under the Original and the reply, so its view,
            // its place, caret and selection survive the switch (ADR 0110).
            ZStack {
                textPane
                    .opacity(textShows ? 1 : 0)
                    .allowsHitTesting(textShows)
                    .accessibilityHidden(!textShows)
                switch model.pane {
                case .text:
                    EmptyView()
                case .original:
                    OriginalView(original: model.snapshot.original)
                case .reply:
                    if let reply = model.snapshot.reply {
                        ReplyPane(reply: reply, onClear: model.clearReply)
                    }
                }
            }
        }
    }

    /// The text is in view: always beside the right pane, and on iPhone
    /// while the switch shows it.
    private var textShows: Bool {
        (isRegular && hasRightPane) || model.pane == .text || (model.pane == .reply && model.snapshot.reply == nil)
    }

    /// The text view shows: edit mode, a read document's text, or a read
    /// document with none left (emptied by editing), so typing can go on
    /// (ADR 0109). One branch, so a change of mode keeps the same view
    /// (ADR 0110).
    private var showsTextView: Bool {
        model.isEditing || !model.snapshot.markdown.isEmpty || model.snapshot.phase != nil
    }

    @ViewBuilder
    private var textPane: some View {
        if showsTextView {
            textColumn
        } else if model.importNotFinished {
            ContentUnavailableView(
                "Import not finished",
                systemImage: "doc.badge.clock",
                description: Text("The import stopped before mdoc read the text. Finish import reads it now."))
        } else if !model.snapshot.kind.needsRecognition {
            ContentUnavailableView(
                "No text yet",
                systemImage: "doc.text",
                description: Text("mdoc is reading the file."))
        } else {
            ContentUnavailableView(
                "No text yet",
                systemImage: "text.viewfinder",
                description: model.snapshot.resumesOCR
                    ? Text("Recognition stopped. Finished pages are kept, and resuming reads the rest.")
                    : Text("mdoc reads the text when recognition runs."))
        }
    }

    private var textColumn: some View {
        VStack(spacing: 0) {
            if let warning = model.snapshot.warning {
                Label(warning, systemImage: "exclamationmark.triangle")
                    .font(.footnote)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal)
                    .padding(.vertical, 8)
                    .background(Color.orange.opacity(0.15))
                    .accessibilityIdentifier("import-warning")
            }
            markdownTextView
        }
    }

    private var markdownTextView: some View {
        MarkdownTextView(
            snapshot: model.snapshot,
            selected: model.reviewing,
            scoped: model.scopedMentions,
            reveal: model.review.reveal,
            rendered: !preferences.showsSource,
            serif: preferences.serif,
            onCandidate: { model.open($0) },
            onReplace: { model.markSelection($0) },
            onReviewNext: { model.reviewNext() },
            onMenu: { model.menuOpened(on: $0) },
            menu: { id in CandidateMenu.configuration(for: id, model: model) { sameAsCandidate = $0 } },
            edit: model.isEditing ? EditActions(model) : nil,
            findBar: model.findBarRequest,
            shown: textShows)
    }

    /// Edit is offered while the text shows, in live preview or with Show
    /// Markdown on; Done ends edit mode (ADR 0109).
    private var offersEdit: Bool {
        model.hasEditableText && (isRegular || model.pane == .text)
    }

    /// Done: a tinted checkmark in its own capsule, the confirm style of
    /// iOS 26 (ADR 0110).
    private var doneButton: some View {
        Button {
            model.doneEditing()
        } label: {
            Label("Done editing", systemImage: "checkmark")
                .fontWeight(.semibold)
                // The system's white vanishes on dark mode's near-white ink.
                .foregroundStyle(Color.onAccent)
        }
        .primaryGlass()
        .keyboardShortcut("e", modifiers: .command)
        .accessibilityLabel(Text("Done editing"))
        .accessibilityHint(Text("Back to reviewing names. Your changes are saved."))
        .accessibilityIdentifier("done-editing")
    }

    @ViewBuilder
    private var editButton: some View {
        if offersEdit {
            Button {
                model.enterEditMode()
            } label: {
                Label("Edit", systemImage: "pencil")
            }
            .keyboardShortcut("e", modifiers: .command)
            // Waiting for a job: it says why when tapped (ADR 0109).
            .foregroundStyle(model.editBlockedReason == nil ? Color.accentColor : Color.secondary)
            .accessibilityLabel(Text("Edit text"))
            .accessibilityHint(model.editBlockedReason.map { Text($0) } ?? Text("Edits the document's text."))
            .accessibilityIdentifier("edit-button")
        }
    }

    private enum PickerSource: Identifiable, Hashable {
        case candidate(CandidateID), identity(IdentityID)
        var id: Self { self }
    }

    @ViewBuilder
    private func sameAsPicker(_ source: PickerSource) -> some View {
        switch source {
        case .candidate(let id):
            if let candidate = model.snapshot.candidate(id) {
                SameAsPicker(
                    source: candidate.original, targets: model.sameAsTargets(for: candidate),
                    categoryLabel: candidate.category.label) { target in
                    model.select(id)
                    model.sameAs(target)
                }
            }
        case .identity(let id):
            if let identity = model.snapshot.identities.first(where: { $0.id == id }) {
                SameAsPicker(
                    source: identity.primaryWording,
                    targets: model.sameAsTargets(forIdentity: id, category: identity.category),
                    categoryLabel: identity.category.label) { target in
                    model.link(id, into: target)
                }
            }
        }
    }

    /// The panes the row offers in this window.
    private var panes: [DocumentViewModel.Shown] { model.panes(in: layout) }

    /// The panes' row, pinned under the navigation bar, only with more
    /// than one pane (ADR 0111). In edit mode too: the Original is what
    /// recognized text is checked against.
    @ViewBuilder
    private var panesRow: some View {
        if panes.count > 1 {
            PanesRow(panes: panes, selection: paneBinding, natural: isRegular)
        }
    }

    @ViewBuilder
    private var titleMenu: some View {
        if model.offersFindNamesAgain {
            // Only outside edit mode, the same job as Find names (ADR 0109).
            Button {
                Task { await model.findNamesAgain() }
            } label: {
                Label("Find names again", systemImage: "text.magnifyingglass")
                if let reason = model.findNamesAgainBlockedReason {
                    Text(reason)
                }
            }
            .disabled(model.findNamesAgainBlockedReason != nil)
            .accessibilityHint(Text("Looks for names in the text as it is now, typed text too."))
            .accessibilityIdentifier("find-names-again")
        }
        if model.snapshot.phase == .ready {
            Button {
                model.copyForAI { UIPasteboard.general.string = $0 }
            } label: {
                Label("Copy for AI", systemImage: "doc.on.doc")
            }
        }
        if let app, let document = app.library.document(model.id) {
            DocumentMenuItems(app: app, document: document)
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        if let showSidebar {
            SidebarButton(action: showSidebar)
        }
        // Tapping the title: the document's menu, as in Claude. The title
        // truncates at its tail between ☰ and the trailing buttons, so they
        // keep their place (ADR 0110); the panes have their own row below
        // (ADR 0111).
        ToolbarItem(placement: .principal) {
            Menu {
                titleMenu
            } label: {
                HStack(spacing: Space.xxs) {
                    Text(model.snapshot.title)
                        .font(.headline)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(maxWidth: titleWidth)
                        .fixedSize(horizontal: false, vertical: true)
                    Image(systemName: "chevron.down")
                        .font(.caption2.weight(.bold))
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                }
                .foregroundStyle(.primary)
            }
            .accessibilityLabel(Text(model.snapshot.title))
            .accessibilityHint(Text("Rename, move, share or delete the document."))
            .accessibilityIdentifier("title-menu")
        }
        if model.isEditing {
            // Editing: the checkmark in its own capsule, apart from ⋯, and
            // no Identities (ADR 0110).
            ToolbarItem(placement: .topBarTrailing) {
                doneButton
            }
            ToolbarSpacer(.fixed, placement: .topBarTrailing)
            ToolbarItem(placement: .topBarTrailing) {
                moreMenu
            }
        } else {
            // One group: beside a title menu, separate items overflow into More.
            ToolbarItemGroup(placement: .topBarTrailing) {
                editButton
                Button {
                    model.showIdentities(!model.identitiesShowing)
                } label: {
                    Label("Identities", systemImage: "person.text.rectangle")
                }
                .disabled(model.snapshot.identities.isEmpty && model.snapshot.phase != .ready)
                .accessibilityIdentifier("identities-button")
                moreMenu
            }
        }
    }

    /// The title's widest, so it truncates at its tail in the room between
    /// ☰ and the trailing buttons (ADR 0110). The bar offers a title any
    /// width and centres it, and a toolbar item's place can't be read, so
    /// the room comes from the screen's width less the bar's fixed
    /// buttons: ☰ with its margin, and the trailing capsules (the review
    /// capsule 48 pt a button, edit mode's checkmark and ⋯ capsules), a gap
    /// either side and the chevron. The buttons keep their size at every
    /// text size.
    private var titleWidth: CGFloat {
        let cap: CGFloat = isRegular ? 420 : 300
        guard screenWidth > 0 else { return isRegular ? 420 : 140 }
        let leading: CGFloat = 60
        let trailing: CGFloat = model.isEditing ? 120 : 16 + 14 + 48 * (offersEdit ? 3 : 2)
        return min(cap, max(60, screenWidth - leading - trailing - 2 * Space.xs - 22))
    }

    private var moreMenu: some View {
        Menu {
            Button {
                // Editing: the system find bar, with Replace (ADR 0109).
                if model.isEditing { model.findBarRequest += 1 } else { model.showsFind = true }
            } label: {
                Label("Find in document", systemImage: "magnifyingglass")
            }
            .disabled(model.snapshot.markdown.isEmpty)
            .accessibilityIdentifier("find-in-document")
            // The source as Copy for AI sends it, markers dimmed (ADR 0096).
            Toggle(isOn: Bindable(preferences).showsSource) {
                Label("Show Markdown", systemImage: "number")
            }
            .accessibilityIdentifier("show-markdown")
        } label: {
            Label("More", systemImage: "ellipsis.circle")
        }
        .accessibilityIdentifier("document-menu")
    }

    private var paneBinding: Binding<DocumentViewModel.Shown> {
        Binding(get: { model.shown(in: layout) }, set: { model.show($0) })
    }

    // MARK: Bottom bar

    @ViewBuilder
    private var bottomBar: some View {
        if model.isEditing {
            // Edit mode: no review bar and no Find names (ADR 0109); a
            // notice (the alias hint, what Undo undid) shows above the keys.
            if let notice = model.notice {
                noticeLabel(notice)
                    .padding(.vertical, 6)
            }
        } else if model.isReviewing && model.runningJob == nil && !model.replyCoversText {
            // Not under a reply that covers the text: its bar acts on the
            // reply there (ADR 0111).
            ReviewBar(model: model, detailsHeight: ReviewBarMetrics.detailsHeight(screen: screenHeight))
        } else {
            actionBottomBar
        }
    }

    private func noticeLabel(_ notice: String) -> some View {
        Label(notice, systemImage: model.noticeSymbol)
            .font(.subheadline)
            // Grows with the bar's text, no further (see `layout`).
            .dynamicTypeSize(...DynamicTypeSize.accessibility2)
            .multilineTextAlignment(.center)
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(.thinMaterial, in: Capsule())
            .accessibilityIdentifier("notice")
            .task(id: notice) { await model.noticeShown(notice) }
    }

    private var actionBottomBar: some View {
        VStack(spacing: 8) {
            if let notice = model.notice {
                noticeLabel(notice)
            }
            if let needs = model.setupNeeds, setup.isWorking || !setup.missing(needs).isEmpty {
                // The card's download, of the model this job needs, is the
                // one prominent button; Not now brings Find names back.
                SetupCard(model: setup, needs: needs, dismissible: true, onDismiss: { model.skipSetup() }) {
                    await model.continueAfterSetup()
                }
            } else {
                if let interruption = model.interruption {
                    InterruptionBanner(kind: interruption.kind, isImport: model.importNotFinished) {
                        Task { await model.resume() }
                    }
                }
                if model.showsBelowFloorNote {
                    BelowFloorNote()
                }
                ActionBar(model: model)
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
        .background(.bar)
    }
}
