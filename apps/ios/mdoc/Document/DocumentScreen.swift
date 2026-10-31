import SwiftUI
import UIKit

/// One document: Text and Original switch on iPhone and sit side by side on
/// iPad, with the Identities list beside the text there.
struct DocumentScreen: View {
    @Bindable var model: DocumentViewModel
    let setup: SetupViewModel
    @Environment(\.horizontalSizeClass) private var sizeClass

    private var isRegular: Bool { sizeClass == .regular }

    var body: some View {
        content
            .navigationTitle(model.snapshot.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbar }
            .safeAreaInset(edge: .bottom) { bottomBar }
            .inspector(isPresented: $model.showsIdentities) {
                IdentitiesView(
                    identities: model.snapshot.identities,
                    onOpen: { identity in
                        if let first = model.snapshot.mentions(of: identity).first {
                            if !isRegular { model.showsIdentities = false }
                            model.open(first.id)
                        }
                    },
                    onDecide: { identity, decision in model.decide(identity: identity, decision) },
                    onClose: isRegular ? nil : { model.showsIdentities = false })
                .inspectorColumnWidth(min: 260, ideal: 320, max: 420)
                .presentationDetents([.medium, .large])
            }
            .sheet(item: reviewingBinding) { item in
                if let candidate = model.snapshot.candidate(item.id) {
                    CandidateSheet(
                        candidate: candidate,
                        identity: model.snapshot.identities.first { $0.id == candidate.identityID },
                        hasNext: { scope in model.hasProposal(after: candidate.id, scope: scope) },
                        onDone: { decision, scope in model.decide(candidate.id, decision, scope: scope) },
                        onNext: { model.reviewNext() })
                }
            }
            .sheet(item: manualBinding) { _ in
                ManualReplaceSheet(text: model.manualSelectionText) { category in
                    model.addReplacement(category: category)
                }
            }
            .sheet(isPresented: $model.showsReplySheet) {
                ReplySheet(caseName: model.snapshot.caseName) { reply in
                    model.restore(reply: reply)
                }
            }
            .sheet(item: $model.share, onDismiss: nil) { items in
                ActivityView(items: items.items) { _ in
                    model.shareFinished(items)
                }
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
                model.refresh()
                if isRegular && model.snapshot.phase != .recognizing {
                    model.showsIdentities = true
                }
            }
    }

    // MARK: Panes

    @ViewBuilder
    private var content: some View {
        if isRegular && (model.hasOriginal || model.snapshot.reply != nil) {
            HStack(spacing: 0) {
                textPane
                Divider()
                if model.pane == .reply, let reply = model.snapshot.reply {
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
        MarkdownTextView(
            snapshot: model.snapshot,
            selected: model.reviewing,
            onCandidate: { model.open($0) },
            onReplace: { model.beginReplace($0) },
            onReviewNext: { model.reviewNext() })
        .ignoresSafeArea(.container, edges: .bottom)
    }

    /// The panes a picker offers: on iPhone all of them, on iPad the right side.
    private var panes: [DocumentViewModel.Pane] {
        var panes: [DocumentViewModel.Pane] = isRegular ? [] : [.text]
        if model.hasOriginal || isRegular { panes.append(.original) }
        if model.snapshot.reply != nil { panes.append(.reply) }
        return panes
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        if panes.count > 1 {
            ToolbarItem(placement: .principal) {
                Picker("View", selection: paneBinding) {
                    ForEach(panes, id: \.self) { pane in
                        switch pane {
                        case .text: Text("Text").tag(pane)
                        case .original: Text("Original").tag(pane)
                        case .reply: Text("Reply").tag(pane)
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
                model.showsIdentities.toggle()
            } label: {
                Label("Identities", systemImage: "person.text.rectangle")
            }
            .disabled(model.snapshot.identities.isEmpty && model.snapshot.phase != .ready)
            .accessibilityIdentifier("identities-button")
        }
    }

    private var paneBinding: Binding<DocumentViewModel.Pane> {
        Binding(
            get: { isRegular && model.pane == .text ? .original : model.pane },
            set: { model.pane = $0 })
    }

    // MARK: Bottom bar

    private var bottomBar: some View {
        VStack(spacing: 8) {
            if let notice = model.notice {
                Label(notice, systemImage: "checkmark.circle.fill")
                    .font(.subheadline)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(.thinMaterial, in: Capsule())
                    .accessibilityIdentifier("notice")
                    .task(id: notice) {
                        UIAccessibility.post(notification: .announcement, argument: notice)
                        try? await Task.sleep(for: .seconds(2))
                        model.clearNotice()
                    }
            }
            if model.needsSetup && setup.snapshot.needsSetup {
                // The card's download is the one prominent button; Not now
                // brings Find names back.
                SetupCard(model: setup, dismissible: true, onDismiss: { model.skipSetup() }) {
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

    private var reviewingBinding: Binding<IdentifiedCandidate?> {
        Binding(
            get: { model.reviewing.map(IdentifiedCandidate.init) },
            set: { model.reviewing = $0?.id })
    }

    private var manualBinding: Binding<IdentifiedSelection?> {
        Binding(
            get: { model.manualSelection.map(IdentifiedSelection.init) },
            set: { model.manualSelection = $0?.range })
    }
}

private struct IdentifiedCandidate: Identifiable {
    let id: CandidateID
}

private struct IdentifiedSelection: Identifiable {
    let range: SourceRange
    var id: SourceRange { range }
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
/// Cancel, Replace N, then Share, Copy and Paste reply.
private struct ActionBar: View {
    @Bindable var model: DocumentViewModel
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

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
            primary("Find names", systemImage: "person.text.rectangle") {
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
                    model.reviewNext()
                } label: {
                    Label("Review", systemImage: "list.bullet.rectangle")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                primary("Replace \(count)", systemImage: "eye.slash") {
                    model.replaceAllPending()
                }
            }
        case .handOff where model.pane == .reply && model.snapshot.reply != nil:
            // On the Reply tab the bar acts on the reply (ADR 0087).
            layout {
                Button {
                    model.shareReply()
                } label: {
                    Label("Share reply", systemImage: "square.and.arrow.up")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("share-reply")
                Button {
                    model.copyReply { UIPasteboard.general.string = $0 }
                } label: {
                    Label("Copy reply", systemImage: "doc.on.doc")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("copy-reply")
                pasteReply("Paste another")
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
                pasteReply("Paste reply")
            }
        }
    }

    private func pasteReply(_ title: LocalizedStringKey) -> some View {
        Button {
            model.showsReplySheet = true
        } label: {
            Label(title, systemImage: "arrow.uturn.backward")
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.bordered)
        .accessibilityIdentifier("paste-reply")
    }

    /// The buttons side by side when every label fits on one line, stacked
    /// otherwise: a label never wraps inside its button (ADR 0087).
    private func layout<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        ButtonRow(forceColumn: dynamicTypeSize.isAccessibilitySize) {
            content()
        }
        .lineLimit(1)
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

/// Buttons of equal width in a row when the widest label fits its share, a
/// column otherwise, so a label never wraps or truncates (ADR 0087).
struct ButtonRow: Layout {
    var spacing: CGFloat = 8
    var forceColumn = false

    private func rowShare(_ width: CGFloat?, _ subviews: Subviews) -> CGFloat? {
        guard !forceColumn, !subviews.isEmpty else { return nil }
        let count = CGFloat(subviews.count)
        let widest = subviews.map { $0.sizeThatFits(.unspecified).width }.max() ?? 0
        guard let width else { return widest }
        let share = (width - spacing * (count - 1)) / count
        return widest <= share ? share : nil
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let ideal = subviews.map { $0.sizeThatFits(.unspecified).width }.max() ?? 0
        if let share = rowShare(proposal.width, subviews) {
            let count = CGFloat(subviews.count)
            let height = subviews.map { $0.sizeThatFits(ProposedViewSize(width: share, height: nil)).height }.max() ?? 0
            return CGSize(width: proposal.width ?? share * count + spacing * (count - 1), height: height)
        }
        let width = proposal.width ?? ideal
        let heights = subviews.map { $0.sizeThatFits(ProposedViewSize(width: width, height: nil)).height }
        return CGSize(width: width, height: heights.reduce(0, +) + spacing * CGFloat(max(subviews.count - 1, 0)))
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        if let share = rowShare(bounds.width, subviews) {
            var x = bounds.minX
            for subview in subviews {
                subview.place(at: CGPoint(x: x, y: bounds.minY), proposal: ProposedViewSize(width: share, height: bounds.height))
                x += share + spacing
            }
            return
        }
        var y = bounds.minY
        for subview in subviews {
            let height = subview.sizeThatFits(ProposedViewSize(width: bounds.width, height: nil)).height
            subview.place(at: CGPoint(x: bounds.minX, y: y), proposal: ProposedViewSize(width: bounds.width, height: height))
            y += height + spacing
        }
    }
}
