import MdocFFI
import SwiftUI
import UIKit

/// One document: Text and Original switch on iPhone and sit side by side on
/// iPad, with the Identities list beside the text there. In a narrow iPad
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
            .navigationTitle(model.snapshot.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbar }
            // Above the bottom bar, never over its buttons.
            .toast($model.toast)
            .safeAreaInset(edge: .bottom) { bottomBar }
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { screenHeight = $0 }
            .inspector(isPresented: inspectorBinding) {
                identities
                    .inspectorColumnWidth(min: 260, ideal: 320, max: 420)
                    .reviewSheetDetents()
            }
            .sheet(isPresented: $model.showsFind) {
                FindSheet(model: model)
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
            .onDisappear { model.detachSystemUndo(for: screenID) }
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
            switch model.pane {
            case .text:
                textPane
            case .original:
                OriginalView(original: model.snapshot.original)
            case .reply:
                if let reply = model.snapshot.reply {
                    ReplyPane(reply: reply, onClear: model.clearReply)
                } else {
                    textPane
                }
            }
        }
    }

    @ViewBuilder
    private var textPane: some View {
        if model.snapshot.markdown.isEmpty && model.importNotFinished {
            ContentUnavailableView(
                "Import not finished",
                systemImage: "doc.badge.clock",
                description: Text("The import stopped before mdoc read the text. Finish import reads it now."))
        } else if model.snapshot.markdown.isEmpty && !model.snapshot.kind.needsRecognition {
            ContentUnavailableView(
                "No text yet",
                systemImage: "doc.text",
                description: Text("mdoc is reading the file."))
        } else if model.snapshot.markdown.isEmpty {
            ContentUnavailableView(
                "No text yet",
                systemImage: "text.viewfinder",
                description: model.snapshot.resumesOCR
                    ? Text("Recognition stopped. Finished pages are kept, and resuming reads the rest.")
                    : Text("mdoc reads the text when recognition runs."))
        } else {
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
            menu: { id in CandidateMenu.make(for: id, model: model) { sameAsCandidate = $0 } })
        .ignoresSafeArea(.container, edges: .bottom)
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

    /// What the picker switches between: a pane, or Identities in iPad's
    /// right column.
    private enum Shown: Hashable {
        case pane(DocumentViewModel.Pane)
        case identities
    }

    /// The panes a picker offers: on iPhone all of them, on iPad the right
    /// side, with Identities there in a narrow window.
    private var panes: [Shown] {
        var panes: [Shown] = isRegular ? [] : [.pane(.text)]
        if model.hasOriginal || isRegular { panes.append(.pane(.original)) }
        if identitiesInPane { panes.append(.identities) }
        if model.snapshot.reply != nil { panes.append(.pane(.reply)) }
        return panes
    }

    @ViewBuilder
    private var titleMenu: some View {
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
        if panes.count == 1 {
            // Tapping the title: the document's menu, as in Claude. A title
            // of its own truncates, so Identities and ⋯ keep their place.
            ToolbarItem(placement: .principal) {
                Menu {
                    titleMenu
                } label: {
                    HStack(spacing: Space.xxs) {
                        Text(model.snapshot.title)
                            .font(.headline)
                            .lineLimit(1)
                            .truncationMode(.tail)
                            // Room for ☰ and the trailing buttons on a phone.
                            .frame(maxWidth: isRegular ? 420 : 170)
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
        }
        if panes.count > 1 {
            ToolbarItem(placement: .principal) {
                Picker("View", selection: paneBinding) {
                    ForEach(panes, id: \.self) { shown in
                        switch shown {
                        case .pane(.text): Text("Text").tag(shown)
                        case .pane(.original): Text("Original").tag(shown)
                        case .pane(.reply): Text("Reply").tag(shown)
                        case .identities: Text("Identities").tag(shown)
                        }
                    }
                }
                .pickerStyle(.segmented)
                .fixedSize()
                .accessibilityIdentifier("pane-picker")
            }
        }
        // One group: beside a title menu, separate items overflow into More.
        ToolbarItemGroup(placement: .topBarTrailing) {
            Button {
                model.showIdentities(!model.identitiesShowing)
            } label: {
                Label("Identities", systemImage: "person.text.rectangle")
            }
            .disabled(model.snapshot.identities.isEmpty && model.snapshot.phase != .ready)
            .accessibilityIdentifier("identities-button")
            Menu {
                Button {
                    model.showsFind = true
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
                // The title's menu too, where the title shows Text and Original.
                if panes.count > 1 {
                    Section { titleMenu }
                }
            } label: {
                Label("More", systemImage: "ellipsis.circle")
            }
            .accessibilityIdentifier("document-menu")
        }
    }

    private var paneBinding: Binding<Shown> {
        Binding(
            get: {
                if identitiesInPane && model.wantsIdentities { return .identities }
                return .pane(isRegular && model.pane == .text ? .original : model.pane)
            },
            set: { shown in
                switch shown {
                case .identities:
                    model.showIdentities(true)
                case .pane(let pane):
                    model.show(pane)
                }
            })
    }

    // MARK: Bottom bar

    @ViewBuilder
    private var bottomBar: some View {
        if model.isReviewing && model.runningJob == nil {
            ReviewBar(model: model, detailsHeight: ReviewBarMetrics.detailsHeight(screen: screenHeight))
        } else {
            actionBottomBar
        }
    }

    private var actionBottomBar: some View {
        VStack(spacing: 8) {
            if let notice = model.notice {
                Label(notice, systemImage: model.noticeSymbol)
                    .font(.subheadline)
                    // Grows with the bar's text, no further (see `layout`).
                    .dynamicTypeSize(...DynamicTypeSize.accessibility2)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(.thinMaterial, in: Capsule())
                    .accessibilityIdentifier("notice")
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
