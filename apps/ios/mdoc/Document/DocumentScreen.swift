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
                    onDecide: { identity, decision in model.decide(identity: identity, decision) })
                .inspectorColumnWidth(min: 260, ideal: 320, max: 420)
                .presentationDetents([.medium, .large])
            }
            .sheet(item: reviewingBinding) { item in
                if let candidate = model.snapshot.candidate(item.id) {
                    CandidateSheet(
                        candidate: candidate,
                        identity: model.snapshot.identities.first { $0.id == candidate.identityID },
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
        if model.snapshot.markdown.isEmpty {
            ContentUnavailableView(
                "No text yet",
                systemImage: "text.viewfinder",
                description: Text("mdoc reads the text when recognition runs."))
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
                SetupCard(model: setup, dismissible: false) {
                    await model.continueAfterSetup()
                }
            }
            if let interruption = model.interruption {
                InterruptionBanner(kind: interruption.kind) {
                    Task { await model.resume() }
                }
            }
            ActionBar(model: model)
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
    let resume: () -> Void

    var body: some View {
        HStack {
            // The core cannot resume OCR part-way yet, so both start again.
            Text(kind == .recognize
                ? "Recognition stopped when mdoc left the screen."
                : "Finding names stopped when mdoc left the screen.")
                .font(.footnote)
            Spacer()
            Button("Start again", action: resume)
                .buttonStyle(.bordered)
        }
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
        case .handOff:
            layout {
                Menu {
                    Button {
                        model.share(asFile: false)
                    } label: {
                        Label("Share text", systemImage: "text.alignleft")
                    }
                    Button {
                        model.share(asFile: true)
                    } label: {
                        Label("Share as Markdown file", systemImage: "doc")
                    }
                } label: {
                    Label("Share", systemImage: "square.and.arrow.up")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
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
                Button {
                    model.showsReplySheet = true
                } label: {
                    Label("Paste reply", systemImage: "arrow.uturn.backward")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("paste-reply")
            }
        }
    }

    private func layout<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        let layout = dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(spacing: 8))
            : AnyLayout(HStackLayout(spacing: 8))
        return layout(content)
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
