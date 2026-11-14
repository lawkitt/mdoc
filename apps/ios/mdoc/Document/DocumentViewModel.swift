import Foundation
import MdocFFI
import Observation
import UIKit

/// One document screen. It holds the core's latest snapshot and view state
/// only; every change goes through the session and comes back as a new
/// snapshot.
@MainActor
@Observable
final class DocumentViewModel {
    enum Pane: Hashable {
        case text, original, reply
    }

    /// The header's one primary action, following the document's state.
    enum PrimaryAction: Equatable {
        case recognize(resume: Bool)
        /// An import of a file that needs no recognition stopped before
        /// its text was read (ADR 0088).
        case finishImport
        case findNames
        case running(JobKind, JobUpdate?)
        /// Another document's job holds the one slot.
        case waiting
        case replaceAll(Int)
        case handOff
    }

    let session: any DocumentSession
    /// The device runs the name detector. Below the floor the document
    /// offers no Find names and says why (ADR 0088).
    let findsNames: Bool
    private let jobs: LongJobRunner
    private let storage: MdocStorage
    /// The lists show what changed: a title, a phase.
    let onChange: () -> Void

    private(set) var snapshot: DocumentSnapshot
    /// The review bar: the name under review, its scope, the focus, the
    /// pulled-up view, the hint line and the pending reveal (ADR 0104: one
    /// value with one way to change it). Views read it and send intents.
    private(set) var review: ReviewState
    /// "Same as Maya Bennett · Undo" and the like.
    var toast: ToastMessage?
    /// The window's undo manager while this document shows in it
    /// (ADR 0098's system undo); `attachSystemUndo` sets it.
    weak var systemUndo: UndoManager?
    /// This document's steps on the window's undo stack, put back when it
    /// shows again: the window holds the shown document's steps only (#672).
    var systemUndoSteps = 0
    /// The screen the window's steps belong to (`attachSystemUndo`).
    var systemUndoScreen: UUID?
    var showsFind = false
    /// Fix the words in progress (ADR 0114); nil otherwise. Changed only by
    /// the `+Words` intents, and ended by any other change.
    var wordsFix: WordsFix?
    /// Edit mode (ADR 0109): the keyboard is up on the text, candidates are
    /// dimmed and not tappable, the review bar is hidden. Changed only by
    /// `enterEditMode` and `doneEditing`.
    var isEditing = false
    /// Typing's state between snapshots. Not observed: a keystroke never
    /// draws the screen again.
    @ObservationIgnored var typing = TypingState()
    /// Bumped to show the system find bar over the text (ADR 0109).
    var findBarRequest = 0
    /// Edit mode's link sheet (ADR 0109): the Link key, ⌘K or the link
    /// chip's Edit.
    var linkDraft: LinkDraft?
    /// Try a sample's names, proposed once recognition has read the text.
    var storedNames: [SampleDocument.Name]?
    /// Identities wanted: a regular window shows them in the inspector or
    /// the right column. The user's intent, apart from how it shows
    /// (ADR 0104).
    private(set) var wantsIdentities = false
    /// The right-edge drawer of a compact window is open. Only opening
    /// Identities there opens it: a window that narrows never does (#674).
    private(set) var identitiesDrawerOpen = false
    /// Identities showed in a regular window when edit mode began: they
    /// come back on Done (ADR 0110).
    private var identitiesBeforeEditing = false
    /// The window this document shows in, which `windowChanged` sets.
    private(set) var layout: DocumentLayout = .compact
    /// Where Identities showed when the last change settled; nil before the
    /// document first showed in a regular window.
    private var settledPlacement: IdentitiesPlacement?
    private(set) var pane: Pane = .text
    var showsReplySheet = false
    var share: ShareItems?
    var errorMessage: String?
    /// A short confirmation such as "Copied for AI".
    private(set) var notice: String?
    /// The notice's symbol: a check, or an hourglass while review waits.
    private(set) var noticeSymbol = "checkmark.circle.fill"
    /// Below the floor, where Find names would be: the device cannot find
    /// names, and selecting text marks one (ADR 0088).
    var showsBelowFloorNote: Bool { !findsNames && snapshot.phase == .unscanned }

    /// An import that stopped before its text was read, of a file that is
    /// not a scan or photo: the document says "Import not finished". A PDF
    /// counts even when recognition began on it, since the core starts it
    /// for every PDF once the model is in; Finish import keeps its pages.
    var importNotFinished: Bool {
        snapshot.phase == nil && !snapshot.kind.needsRecognition && runningJob == nil
    }

    /// Recognition or the scan needs a model the device has not downloaded.
    var needsSetup: Bool { waitingForSetup != nil }
    /// The job that asked for setup, run again once the models are in.
    private(set) var waitingForSetup: JobKind?
    /// The model the waiting job needs: recognition needs text recognition,
    /// Find names the name detector (ADR 0090).
    var setupNeeds: ModelPurpose? {
        switch waitingForSetup {
        case .recognize: .ocr
        case .scan: .detector
        case .install, nil: nil
        }
    }

    init(
        session: any DocumentSession, jobs: LongJobRunner, storage: MdocStorage, findsNames: Bool = true,
        onChange: @escaping () -> Void = {}
    ) {
        self.session = session
        self.findsNames = findsNames
        self.jobs = jobs
        self.storage = storage
        self.onChange = onChange
        let snapshot = session.snapshot()
        self.snapshot = snapshot
        review = ReviewState(snapshot)
    }

    var id: DocumentID { snapshot.id }

    var hasOriginal: Bool { snapshot.original != .none }

    var runningJob: RunningJob? {
        guard let job = jobs.current, job.documentID == id else { return nil }
        return job
    }

    /// A job of this document stopped when mdoc left the screen.
    var interruption: InterruptedJob? {
        guard let job = jobs.interrupted, job.documentID == id else { return nil }
        return job
    }

    var primaryAction: PrimaryAction {
        if let job = runningJob {
            return .running(job.kind, job.progress)
        }
        switch snapshot.phase {
        case .unscanned where !findsNames:
            // Nothing to find here: marking by selection, then the hand-off.
            // A case's names can still come in as proposals on import.
            return snapshot.pendingCount > 0 ? .replaceAll(snapshot.pendingCount) : .handOff
        case nil where jobs.current != nil, .unscanned where jobs.current != nil:
            return .waiting
        case nil where importNotFinished:
            return .finishImport
        case nil:
            return .recognize(resume: snapshot.pagesDone > 0 || snapshot.resumesOCR)
        case .unscanned:
            return .findNames
        case .reviewing:
            return .replaceAll(snapshot.pendingCount)
        case .ready:
            return .handOff
        }
    }

    // MARK: Jobs

    func recognize() async {
        waitingForSetup = nil
        let session = session
        let outcome = await jobs.run(.recognize, documentID: id, title: String(localized: "Recognizing \(snapshot.title)")) { observer in
            try session.recognize(observer: observer)
        }
        if case .failed(CoreError.modelMissing) = outcome {
            // Scans, photos and text-less PDFs need OCR.
            waitingForSetup = .recognize
            return
        }
        finish(outcome)
    }

    func findNames() async {
        waitingForSetup = nil
        let session = session
        let outcome = await jobs.run(.scan, documentID: id, title: String(localized: "Finding names in \(snapshot.title)")) { observer in
            try session.scan(observer: observer)
        }
        if case .failed(CoreError.modelMissing) = outcome {
            waitingForSetup = .scan
            return
        }
        finish(outcome)
        // A scan that finds nothing says so, rather than going quietly to
        // Share (ADR 0087).
        if case .finished(let scanned) = outcome {
            let proposed = scanned.pendingCount
            if scanned.candidates.isEmpty {
                say(String(localized: "No names found. Select text to mark one."))
            } else if proposed > 0 {
                // Where to look next, the first time a reader sees the colours.
                say(String(localized: "\(proposed) to review: tap underlined text"), symbol: "text.magnifyingglass")
            }
        }
    }

    /// Download & continue: the job that asked for setup runs again.
    /// Not now on the document's setup card: Find names (or Recognize)
    /// returns as the primary action.
    func skipSetup() {
        waitingForSetup = nil
    }

    func continueAfterSetup() async {
        switch waitingForSetup {
        case .recognize: await recognize()
        case .scan: await findNames()
        case .install, nil: break
        }
    }

    /// Resume after mdoc left the screen: OCR keeps its finished pages, the
    /// scan reruns.
    func resume() async {
        guard let interruption else { return }
        jobs.dismissInterruption()
        switch interruption.kind {
        case .recognize: await recognize()
        case .scan: await findNames()
        case .install: break
        }
    }

    func cancelJob() {
        jobs.cancel()
    }

    private func finish(_ outcome: JobOutcome<DocumentSnapshot>) {
        switch outcome {
        case .finished(let snapshot):
            applyReview(snapshot)
            if let names = storedNames, snapshot.phase != nil {
                proposeStored(names)
            }
        case .cancelled:
            applyReview(session.snapshot())
        case .failed(let error):
            errorMessage = error.localizedDescription
            applyReview(session.snapshot())
        case .busy:
            errorMessage = String(localized: "Another job is running. Try again when it finishes.")
        }
    }

    // MARK: Review

    /// Review waits while this document's job runs: a scan whose text
    /// changed meanwhile is thrown away at the end (the core's rule, as on
    /// desktop), so a Replace now would cost the whole Find names. Says so
    /// and returns true when it waits.
    func reviewWaitsForJob() -> Bool {
        guard let job = runningJob else { return false }
        say(
            job.kind == .scan
                ? String(localized: "Finding names. Review when it finishes.")
                : String(localized: "Reading the document. Review when it finishes."),
            symbol: "hourglass")
        return true
    }

    /// A tap on a candidate: the review bar opens on it, or retargets in
    /// place when it is already up (ADR 0098).
    func open(_ candidate: CandidateID) {
        guard !reviewWaitsForJob() else { return }
        showTextForReview()
        select(candidate)
    }

    /// The first proposal after the one under review, wrapping around.
    func reviewNext() {
        updateReview { $0.reviewNext() }
    }

    func decide(_ candidate: CandidateID, _ decision: Decision, scope: Scope) {
        guard !reviewWaitsForJob() else { return }
        perform { try session.decide(candidate, decision, scope: scope) }
    }

    func decide(identity: IdentityID, _ decision: Decision) {
        guard let first = snapshot.mentions(of: identity).first else { return }
        decide(first.id, decision, scope: .all)
    }

    func replaceAllPending() {
        guard !reviewWaitsForJob() else { return }
        let count = snapshot.pendingCount
        let wasReviewing = isReviewing
        perform { try session.replaceAllPending() }
        // One tap changed them all: says so, with Undo (ADR 0098).
        if errorMessage == nil && count > 0 {
            if wasReviewing { endReview() }
            toast = ToastMessage(
                text: String(localized: "\(count) replaced"),
                actionTitle: String(localized: "Undo"), action: { [weak self] in self?.undoTapped() })
        }
    }

    // MARK: Hand-off

    /// Copy for AI to the clipboard.
    func copyForAI(to pasteboard: (String) -> Void) {
        do {
            let handOff = try session.copyForAI()
            pasteboard(handOff.text)
            say(String(localized: "Copied for AI"))
            onChange()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Share Copy for AI's text, as text or as a `.md` file. Save to Files
    /// gets the file either way. The file stays until the next share or
    /// launch: an app such as Telegram reads it after the sheet closes.
    func share(asFile: Bool) {
        do {
            let handOff = try session.copyForAI()
            storage.clear(.handOff)
            let url = try storage.write(Data(handOff.text.utf8), named: Self.safeFileName(handOff.fileName), in: .handOff)
            if asFile {
                share = ShareItems(items: [url])
            } else {
                share = ShareItems(items: [TitledText(text: handOff.text, title: snapshot.title, file: url)])
            }
            onChange()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func clearNotice() {
        notice = nil
    }

    func say(_ text: String, symbol: String = "checkmark.circle.fill") {
        notice = text
        noticeSymbol = symbol
    }

    /// The notice is on screen: VoiceOver says it, and it goes once it has
    /// been up long enough to read. The screen runs this while the notice
    /// shows, so a notice said while the review bar covers it, or before the
    /// document shows, waits to be seen; a newer notice cancels the wait.
    func noticeShown(_ text: String) async {
        UIAccessibility.post(notification: .announcement, argument: text)
        // timing: how long a notice stays to be read, not a wait for an event (ADR 0099)
        do { try await Task.sleep(for: Self.noticeDuration(text)) } catch { return }
        if notice == text { notice = nil }
    }

    /// How long a notice stays: two seconds, up to four for a sentence.
    static func noticeDuration(_ text: String) -> Duration {
        .seconds(min(4, max(2, Double(text.count) / 12)))
    }

    static func safeFileName(_ name: String) -> String {
        let cleaned = name.components(separatedBy: CharacterSet(charactersIn: "/:\\")).joined(separator: "-")
        return cleaned.isEmpty ? "document.md" : cleaned
    }

    // MARK: Restore

    /// Paste reply: restores it with this document's case and opens it.
    func restore(reply: String) {
        let trimmed = reply.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        perform { try session.restoreReply(reply) }
        if snapshot.reply != nil {
            showsReplySheet = false
            show(.reply)
        }
    }

    /// The restored reply, as the hand-off bar copies it on the Reply tab.
    func copyReply(to pasteboard: (String) -> Void) {
        guard let reply = snapshot.reply else { return }
        pasteboard(reply.restored.text)
        say(String(localized: "Copied reply"))
    }

    func shareReply() {
        guard let reply = snapshot.reply else { return }
        share = ShareItems(items: [TitledText(text: reply.restored.text, title: snapshot.title)])
    }

    func clearReply() {
        perform { try session.clearReply() }
        if pane == .reply {
            pane = .text
        }
    }

    // MARK: Identities

    /// iPad's right pane beside the text: the original or the reply.
    var hasRightPane: Bool { hasOriginal || snapshot.reply != nil }

    /// Where Identities shows in this window (`DocumentLayout`).
    var identitiesPlacement: IdentitiesPlacement {
        layout.identities(hasRightPane: hasRightPane)
    }

    /// Identities show now, wherever they show.
    var identitiesShowing: Bool {
        identitiesPlacement == .drawer ? identitiesDrawerOpen : wantsIdentities
    }

    /// Opens or closes Identities as the user asked; in a compact window
    /// that is the drawer. In the right column they cover the reply, so the
    /// hand-off bar goes back to acting on the document, the text in view.
    func showIdentities(_ show: Bool) {
        // Hidden while editing (ADR 0110): no button, no edge swipe.
        if show && isEditing { return }
        let placement = identitiesPlacement
        if show && placement == .column && pane == .reply { pane = .text }
        wantsIdentities = show
        identitiesDrawerOpen = show && placement == .drawer
    }

    /// Shows `pane`. In the right column a pane takes Identities' place.
    func show(_ pane: Pane) {
        if identitiesPlacement == .column {
            wantsIdentities = false
            // A pane chosen while editing keeps the column: Done leaves it.
            if isEditing && pane != .text { identitiesBeforeEditing = false }
        }
        self.pane = pane
    }

    /// The document shows in a window of `layout`: on appearing, after a
    /// rotation, a size-class change or a narrowing window.
    func windowChanged(to layout: DocumentLayout) {
        self.layout = layout
        settleIdentities()
    }

    /// Where Identities shows changed (the window, or a right pane coming
    /// or going): the drawer closes, so a window that narrows never opens
    /// it over the text (#674). Entering a regular window shows the
    /// inspector unless the document waits for recognition; turning between
    /// the inspector and the right column (a rotation, a reply restored)
    /// shows the original in the column and the list in the inspector.
    private func settleIdentities() {
        let placement = identitiesPlacement
        guard placement != settledPlacement else { return }
        defer { settledPlacement = placement }
        identitiesDrawerOpen = false
        switch (settledPlacement, placement) {
        case (_, .drawer):
            break
        case (nil, .inspector), (.drawer, .inspector):
            if snapshot.phase != nil { wantsIdentities = true }
        case (nil, .column), (.drawer, .column):
            break
        case (_, .inspector):
            wantsIdentities = true
        case (_, .column):
            wantsIdentities = false
        }
        if isEditing && wantsIdentities {
            // Wanted, but hidden until Done (ADR 0110).
            identitiesBeforeEditing = true
            wantsIdentities = false
        }
    }

    /// Edit mode begins: Identities go, the drawer closed and the iPad
    /// column or inspector given back to the Original (ADR 0110).
    func hideIdentitiesForEditing() {
        identitiesBeforeEditing = wantsIdentities && identitiesPlacement != .drawer
        wantsIdentities = false
        identitiesDrawerOpen = false
    }

    /// Done: Identities come back where they showed in a regular window.
    /// The iPhone drawer stays shut, over the text it would cover.
    func restoreIdentitiesAfterEditing() {
        defer { identitiesBeforeEditing = false }
        guard identitiesBeforeEditing, identitiesPlacement != .drawer else { return }
        showIdentities(true)
    }

    // MARK: Helpers

    func refresh() {
        applyReview(session.snapshot())
    }

    /// The one change runner: runs a core change, shows the new snapshot
    /// (the review bar follows it) and, when the change made an undo step,
    /// mirrors it on the window's undo stack. A failure shows its message
    /// and returns false; with `rethrowing` it is handed back instead, to a
    /// caller that shows it its own way (Rename: an alias taken offers Same
    /// as). `undoable` is false for changes that make no step of their own
    /// (undo and redo, a dismissed hint, stored proposals).
    @discardableResult
    func change(
        undoable: Bool = true, rethrowing: Bool = false, _ body: () throws -> DocumentSnapshot
    ) throws -> Bool {
        let before = snapshot.undoStep
        do {
            applyReview(try body())
        } catch {
            if rethrowing { throw error }
            errorMessage = error.localizedDescription
            return false
        }
        if undoable && snapshot.canUndo && snapshot.undoStep != before { registerSystemUndo() }
        return true
    }

    /// `change` for a caller that lets it show its errors: it never throws.
    @discardableResult
    func perform(undoable: Bool = true, _ body: () throws -> DocumentSnapshot) -> Bool {
        (try? change(undoable: undoable, body)) ?? false
    }

    /// One transition of the review bar, given the hints still to teach.
    /// Written only when it changed, so views reading the bar redraw only
    /// then.
    @discardableResult
    func updateReview<T>(_ transition: (inout ReviewState) -> T) -> T {
        var state = review
        state.setTeachable(ReviewHint.teachable)
        let result = transition(&state)
        if state != review { review = state }
        return result
    }

    func applyReview(_ new: DocumentSnapshot) {
        // The draft was of the text before this change; a snapshot of the
        // same text and history (discovery, a job's tick) keeps it.
        if new.textRevision != snapshot.textRevision || new.undoStep != snapshot.undoStep { wordsFix = nil }
        snapshot = new
        // The snapshot is the core's latest: typing's own answers are older.
        typing.settled(at: new)
        var state = review
        state.apply(new)
        if state != review { review = state }
        if new.reply == nil && pane == .reply {
            pane = .text
        }
        settleIdentities()
        onChange()
    }
}
