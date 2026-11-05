import SwiftUI
import TipKit
import UIKit

/// One document: Text and Original switch on iPhone and sit side by side on
/// iPad, with the Identities list beside the text there. In a narrow iPad
/// window (portrait) Identities take the Original's place instead of a third
/// column (ADR 0087).
struct DocumentScreen: View {
    @Bindable var model: DocumentViewModel
    let setup: SetupViewModel
    /// The window is too narrow for a third column (iPad portrait).
    var isNarrow = false
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.undoManager) private var undoManager
    @AppStorage(MarkdownDisplay.showsSourceKey, store: MarkdownDisplay.store) private var showsSource = false
    @AppStorage(MarkdownDisplay.serifKey, store: MarkdownDisplay.store) private var serif = false

    private var isRegular: Bool { sizeClass == .regular }
    @State private var screenHeight: CGFloat = 800
    /// The long press's Same as… with a long list: the searchable picker.
    @State private var sameAsCandidate: CandidateID?
    /// Identities' Same as…: the identity it starts from.
    @State private var sameAsIdentity: IdentityID?

    /// iPad's right column: the original or the reply beside the text.
    private var hasRightPane: Bool { model.hasOriginal || model.snapshot.reply != nil }

    /// Identities show in the right column rather than as an inspector.
    private var identitiesInPane: Bool { isRegular && isNarrow && hasRightPane }

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
                    // Shown even when the Reply tab was already selected.
                    if identitiesInPane && model.pane == .reply { model.showsIdentities = false }
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
                model.systemUndo = undoManager
                undoManager?.levelsOfUndo = 256
                model.refresh()
                // In a narrow window the original shows first.
                if isRegular && !identitiesInPane && model.snapshot.phase != .recognizing {
                    model.showsIdentities = true
                }
            }
            .onChange(of: identitiesInPane) { _, inPane in
                // Rotating to portrait shows the original; to landscape, the list.
                if isRegular { model.showsIdentities = !inPane }
            }
            .onChange(of: model.pane) { _, pane in
                // A restored reply shows over the list.
                if identitiesInPane && pane == .reply { model.showsIdentities = false }
            }
    }

    private var identities: some View {
        IdentitiesView(
            identities: model.snapshot.identities,
            onOpen: { identity in
                // The bar on the identity's first undecided mention, or its
                // first (ADR 0098's map).
                let mentions = model.snapshot.mentions(of: identity).sorted { $0.range.start < $1.range.start }
                if let first = mentions.first(where: { $0.state == .proposed }) ?? mentions.first {
                    if !isRegular { model.showsIdentities = false }
                    model.open(first.id)
                }
            },
            onDecide: { identity, decision in model.decide(identity: identity, decision) },
            onFocus: { focus in
                if !isRegular { model.showsIdentities = false }
                model.beginReview(focus: focus)
            },
            onSameAs: { identity, target in model.link(identity, into: target) },
            onPickSameAs: { identity in sameAsIdentity = identity },
            mentions: { identity in
                model.snapshot.mentions(of: identity).sorted { $0.range.start < $1.range.start }
                    .map { (id: $0.id, context: model.context(of: $0), state: $0.state) }
            },
            onOpenMention: { id in
                if !isRegular { model.showsIdentities = false }
                model.open(id)
            },
            onClose: isRegular ? nil : { model.showsIdentities = false })
    }

    /// Identities in the right column cover the reply, so the hand-off bar
    /// goes back to acting on the document, the text the user sees.
    private func showIdentities(_ show: Bool) {
        if show && identitiesInPane && model.pane == .reply { model.pane = .text }
        model.showsIdentities = show
    }

    /// The inspector stays closed while Identities show in the right column.
    private var inspectorBinding: Binding<Bool> {
        Binding(
            get: { model.showsIdentities && !identitiesInPane },
            set: { model.showsIdentities = $0 })
    }

    // MARK: Panes

    @ViewBuilder
    private var content: some View {
        if isRegular && hasRightPane {
            HStack(spacing: 0) {
                textPane
                Divider()
                if identitiesInPane && model.showsIdentities {
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
                markdownText
            }
        }
    }

    private var markdownText: some View {
        VStack(spacing: 0) {
            if model.snapshot.pendingCount > 0 && !model.isReviewing {
                TipView(LongPressTip())
                    .padding(.horizontal)
            }
            markdownTextView
        }
    }

    private var markdownTextView: some View {
        MarkdownTextView(
            snapshot: model.snapshot,
            selected: model.reviewing,
            scoped: model.scopedMentions,
            revealRequest: model.revealRequest,
            revealRange: model.revealRange,
            rendered: !showsSource,
            serif: serif,
            onCandidate: { model.open($0) },
            onReplace: { model.markSelection($0) },
            onReviewNext: { model.reviewNext() },
            menu: { id in candidateMenu(id) })
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

    /// A long press on a name (ADR 0098): what its state allows, under a
    /// header with the name and its alias. Each action puts the bar on it.
    private func candidateMenu(_ id: CandidateID) -> UIMenu? {
        guard let candidate = model.snapshot.candidate(id) else { return nil }
        func act(_ scope: Scope = .allOccurrences, _ body: @escaping () -> Void) -> () -> Void {
            { model.select(id); model.reviewScope = scope; body() }
        }
        func action(_ title: String, _ image: String, attributes: UIMenuElement.Attributes = [], _ run: @escaping () -> Void) -> UIAction {
            UIAction(title: title, image: UIImage(systemName: image), attributes: attributes) { _ in run() }
        }
        let categories = UIMenu(
            title: String(localized: "Change category"), image: UIImage(systemName: "tag"),
            children: Category.allCases.filter { $0 != candidate.category }.map { category in
                action(category.label, category.symbol, act { model.setCategory(category) })
            })
        let found = model.sameAsTargets(for: candidate)
        let targets = found.all
        func targetActions(_ identities: [Identity]) -> [UIMenuElement] {
            identities.map { target in
                action("\(target.primaryWording) · \(target.alias)", target.category.symbol, act { model.sameAs(target.id) })
            }
        }
        // The same order as the card's picker; past 8, its searchable sheet.
        let sameAs: UIMenuElement = targets.count > 8
            ? action(String(localized: "Same as…"), "link", act { sameAsCandidate = id })
            : UIMenu(title: String(localized: "Same as…"), image: UIImage(systemName: "link"), children: [
                UIMenu(title: String(localized: "Looks like"), options: .displayInline, children: targetActions(found.hinted)),
                UIMenu(options: .displayInline, children: targetActions(found.sameCategory)),
                UIMenu(title: String(localized: "Other categories"), children: targetActions(found.otherCategories)),
            ].filter { !$0.children.isEmpty })
        var children: [UIMenuElement] = []
        switch candidate.state {
        case .proposed:
            children = [
                action(String(localized: "Replace"), "eye.slash", act { model.decideCurrent(.replace) }),
                action(String(localized: "Keep"), "eye", act { model.decideCurrent(.keep) }),
                action(String(localized: "Just this one"), "1.circle", act(.justThisOne) { model.decideCurrent(.replace) }),
            ]
            if !targets.isEmpty { children.append(sameAs) }
            children.append(categories)
        case .replaced:
            children = [
                action(String(localized: "Show original"), "text.quote") {
                    model.toast = ToastMessage(text: String(localized: "Original: \(candidate.original)"))
                },
                action(String(localized: "Keep instead"), "eye", act { model.decideCurrent(.keep) }),
            ]
            if !targets.isEmpty { children.append(sameAs) }
            children.append(categories)
        case .kept:
            children = [
                action(String(localized: "Replace instead"), "eye.slash", act { model.decideCurrent(.replace) }),
                categories,
            ]
        }
        return UIMenu(title: "\(candidate.original) → \(candidate.alias)", children: children)
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

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
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
        ToolbarItem(placement: .primaryAction) {
            Button {
                showIdentities(!model.showsIdentities)
            } label: {
                Label("Identities", systemImage: "person.text.rectangle")
            }
            .disabled(model.snapshot.identities.isEmpty && model.snapshot.phase != .ready)
            .accessibilityIdentifier("identities-button")
        }
        ToolbarItem(placement: .primaryAction) {
            Menu {
                Button {
                    model.showsFind = true
                } label: {
                    Label("Find in document", systemImage: "magnifyingglass")
                }
                .disabled(model.snapshot.markdown.isEmpty)
                .accessibilityIdentifier("find-in-document")
                // The source as Copy for AI sends it, markers dimmed (ADR 0096).
                Toggle(isOn: $showsSource) {
                    Label("Show Markdown", systemImage: "number")
                }
                .accessibilityIdentifier("show-markdown")
            } label: {
                Label("More", systemImage: "ellipsis.circle")
            }
            .accessibilityIdentifier("document-menu")
        }
    }

    private var paneBinding: Binding<Shown> {
        Binding(
            get: {
                if identitiesInPane && model.showsIdentities { return .identities }
                return .pane(isRegular && model.pane == .text ? .original : model.pane)
            },
            set: { shown in
                switch shown {
                case .identities:
                    showIdentities(true)
                case .pane(let pane):
                    if identitiesInPane { model.showsIdentities = false }
                    model.pane = pane
                }
            })
    }

    // MARK: Bottom bar

    @ViewBuilder
    private var bottomBar: some View {
        if model.isReviewing && model.runningJob == nil {
            ReviewBar(model: model, detailsHeight: max(120, screenHeight * 0.45 - 170))
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
                    .task(id: notice) {
                        UIAccessibility.post(notification: .announcement, argument: notice)
                        // Long enough to read: two seconds, more for a sentence.
                        // A newer notice cancels this wait and must stay.
                        do {
                            try await Task.sleep(for: .seconds(min(4, max(2, Double(notice.count) / 12))))
                            model.clearNotice()
                        } catch {}
                    }
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

/// "Recognition stopped when mdoc left the screen."
private struct InterruptionBanner: View {
    let kind: JobKind
    /// The stopped job read a file that needs no recognition.
    let isImport: Bool
    let resume: () -> Void

    var body: some View {
        HStack {
            // OCR keeps its finished pages and resumes; the scan reruns.
            Text(message)
                .font(.footnote)
            Spacer()
            Button(button, action: resume)
                .buttonStyle(.bordered)
                .accessibilityIdentifier("interruption-resume")
        }
    }

    private var message: LocalizedStringKey {
        switch kind {
        case .recognize where isImport: "The import stopped when mdoc left the screen."
        case .recognize: "Recognition stopped when mdoc left the screen. Finished pages are kept."
        case .scan, .install: "Finding names stopped when mdoc left the screen."
        }
    }

    private var button: LocalizedStringKey {
        switch kind {
        case .recognize where isImport: "Finish import"
        case .recognize: "Resume"
        case .scan, .install: "Start again"
        }
    }
}

/// Below the detector floor, where Find names would be (ADR 0088).
private struct BelowFloorNote: View {
    var body: some View {
        Label(BelowFloor.note, systemImage: "info.circle")
            .font(.footnote)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("below-floor-note")
    }
}

/// The primary action, as on desktop: Recognize, Find names, running with
/// Cancel, Replace N, then Share, Copy and Paste (a reply).
private struct ActionBar: View {
    @Bindable var model: DocumentViewModel

    var body: some View {
        switch model.primaryAction {
        case .recognize(let resume):
            primary(resume ? "Resume recognition" : "Recognize text", systemImage: "text.viewfinder") {
                Task { await model.recognize() }
            }
        case .finishImport:
            primary("Finish import", systemImage: "doc.badge.arrow.up") {
                Task { await model.recognize() }
            }
        case .findNames:
            primary("Find names", systemImage: "text.magnifyingglass") {
                Task { await model.findNames() }
            }
        case .running(let kind, let progress):
            HStack(spacing: 12) {
                Group {
                    // The core reports stages; a fraction only when it knows one.
                    if let fraction = progress?.fraction {
                        ProgressView(value: fraction) {
                            Text(Self.progressLabel(kind, progress))
                                .font(.footnote)
                        }
                    } else {
                        HStack(spacing: 8) {
                            ProgressView()
                            Text(Self.progressLabel(kind, progress))
                                .font(.footnote)
                            Spacer(minLength: 0)
                        }
                        .accessibilityElement(children: .combine)
                    }
                }
                .accessibilityIdentifier("job-progress")
                Button("Cancel", role: .cancel) {
                    model.cancelJob()
                }
                .buttonStyle(.bordered)
            }
        case .waiting:
            Text("Waiting for another document to finish.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity)
        case .replaceAll(let count):
            layout {
                Button {
                    model.beginReview()
                } label: {
                    Label("Review", systemImage: "list.bullet.rectangle")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                primary("Replace \(count)", systemImage: "eye.slash") {
                    model.replaceAllPending()
                }
                .accessibilityLabel(Text("Replace all \(count) proposals"))
                .accessibilityHint(Text("Each gets its alias. You can still keep any of them."))
            }
        case .handOff where model.pane == .reply && model.snapshot.reply != nil:
            // On the Reply tab the bar acts on the reply (ADR 0087). Short
            // labels keep three buttons in one row on a phone; VoiceOver
            // says what each acts on.
            layout {
                Button {
                    model.shareReply()
                } label: {
                    Label("Share", systemImage: "square.and.arrow.up")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .accessibilityLabel(Text("Share reply"))
                .accessibilityIdentifier("share-reply")
                Button {
                    model.copyReply { RestoredPasteboard.copy($0) }
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .accessibilityLabel(Text("Copy reply"))
                .accessibilityIdentifier("copy-reply")
                pasteReply(accessibilityLabel: "Paste another reply")
            }
        case .handOff:
            layout {
                // One tap shares the text; the file is a long press away.
                Button {
                    model.share(asFile: false)
                } label: {
                    Label("Share", systemImage: "square.and.arrow.up")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .contextMenu {
                    Button {
                        model.share(asFile: true)
                    } label: {
                        Label("Share as Markdown file", systemImage: "doc")
                    }
                }
                .accessibilityAction(named: Text("Share as Markdown file")) {
                    model.share(asFile: true)
                }
                .accessibilityIdentifier("primary-action")
                Button {
                    model.copyForAI { UIPasteboard.general.string = $0 }
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .accessibilityHint(Text("Copies the text for an AI app, with the note about placeholders."))
                .accessibilityIdentifier("copy-for-ai")
                pasteReply(accessibilityLabel: "Paste reply")
            }
        }
    }

    /// Paste, which opens the reply sheet.
    private func pasteReply(accessibilityLabel: LocalizedStringKey) -> some View {
        Button {
            model.showsReplySheet = true
        } label: {
            Label("Paste", systemImage: "arrow.uturn.backward")
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.bordered)
        .accessibilityLabel(Text(accessibilityLabel))
        .accessibilityIdentifier("paste-reply")
    }

    /// The buttons side by side when every label fits on one line, stacked
    /// otherwise: a label never wraps inside its button (ADR 0087). Like the
    /// system's own bars, the bar's text stops growing at the second
    /// accessibility size, where the primary button keeps a row and the
    /// others share one, so the text keeps most of the screen; a long press
    /// shows a label in the large content viewer.
    private func layout<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        ButtonRow {
            Group { content() }
                .accessibilityShowsLargeContentViewer()
        }
        .lineLimit(1)
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
    }

    private func primary(_ title: LocalizedStringKey, systemImage: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .accessibilityIdentifier("primary-action")
    }

    static func progressLabel(_ kind: JobKind, _ progress: JobUpdate?) -> String {
        if let page = progress?.page, let count = progress?.pageCount, count > 0 {
            return String(localized: "Recognizing page \(min(page + 1, count)) of \(count)")
        }
        switch progress?.stage {
        case .reading: return String(localized: "Reading the file")
        case .converting: return String(localized: "Converting")
        case .recognizing: return String(localized: "Recognizing text")
        case .scanning: return String(localized: "Finding names")
        case .installing: return String(localized: "Installing")
        case nil:
            switch kind {
            case .recognize: return String(localized: "Reading the file")
            case .scan: return String(localized: "Finding names")
            case .install: return String(localized: "Installing")
            }
        }
    }
}

/// Buttons of equal width in a row when the widest label fits its share.
/// Otherwise the first button (the primary one) takes a row of its own and
/// the rest share the next when they fit, else every button takes a row: a
/// label never wraps or truncates (ADR 0087).
struct ButtonRow: Layout {
    var spacing: CGFloat = 8
    /// One button a row, always.
    var forceColumn = false

    /// The rows: how many buttons each holds, first row first.
    private func rows(_ width: CGFloat?, _ subviews: Subviews) -> [Int] {
        let count = subviews.count
        guard count > 1 else { return count == 1 ? [1] : [] }
        if forceColumn { return Array(repeating: 1, count: count) }
        let widths = subviews.map { $0.sizeThatFits(.unspecified).width }
        guard let width else { return [count] }
        if fits(widths, in: width) { return [count] }
        if count > 2, fits(Array(widths.dropFirst()), in: width) { return [1, count - 1] }
        return Array(repeating: 1, count: count)
    }

    private func fits(_ widths: [CGFloat], in width: CGFloat) -> Bool {
        let share = (width - spacing * CGFloat(widths.count - 1)) / CGFloat(widths.count)
        return (widths.max() ?? 0) <= share
    }

    /// Each row's buttons with their frames' width.
    private func arranged(_ width: CGFloat, _ subviews: Subviews) -> [(Subviews, CGFloat)] {
        var start = subviews.startIndex
        return rows(width, subviews).map { count in
            let row = subviews[start..<start + count]
            start += count
            return (row, (width - spacing * CGFloat(count - 1)) / CGFloat(count))
        }
    }

    private func height(of row: Subviews, share: CGFloat) -> CGFloat {
        row.map { $0.sizeThatFits(ProposedViewSize(width: share, height: nil)).height }.max() ?? 0
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let widest: CGFloat = subviews.map { $0.sizeThatFits(.unspecified).width }.max() ?? 0
        let gaps = CGFloat(max(subviews.count - 1, 0))
        let width: CGFloat = proposal.width ?? widest * CGFloat(subviews.count) + spacing * gaps
        let rows = arranged(width, subviews)
        var total: CGFloat = spacing * CGFloat(max(rows.count - 1, 0))
        for (row, share) in rows {
            total += height(of: row, share: share)
        }
        return CGSize(width: width, height: total)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for (row, share) in arranged(bounds.width, subviews) {
            let rowHeight = height(of: row, share: share)
            var x = bounds.minX
            for subview in row {
                subview.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(width: share, height: rowHeight))
                x += share + spacing
            }
            y += rowHeight + spacing
        }
    }
}
