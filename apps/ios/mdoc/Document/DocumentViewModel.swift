import Foundation
import MdocFFI
import Observation

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
    private let onChange: () -> Void

    private(set) var snapshot: DocumentSnapshot
    /// The candidate on the review bar; nil when not reviewing (ADR 0098).
    var reviewing: CandidateID?
    /// The bar's scope chip: All by default, held within an identity and
    /// reset to All on the next one.
    var reviewScope: Scope = .all
    /// The identity the scope was chosen for.
    var scopeIdentity: IdentityID?
    /// Pulled up: the identity's mentions, one row each.
    var barExpanded = false
    /// The one hint line this document shows, while it shows.
    var hint: ReviewHint?
    /// A hint has shown in this document: no second one.
    var hintShownHere = false
    /// Cards and decisions so far in this document, which time the hints.
    var cardsSeen = 0
    var decisionsHere = 0
    /// Stepping limited to one identity or category.
    var focus: ReviewFocus?
    /// "Same as Maya Bennett · Undo" and the like.
    var toast: ToastMessage?
    /// Bumped to scroll the text back to the candidate under review, or to
    /// `revealRange` (a Find match).
    var revealRequest = 0
    var revealRange: SourceRange?
    /// The window's undo manager while this document shows in it
    /// (ADR 0098's system undo); `attachSystemUndo` sets it.
    weak var systemUndo: UndoManager?
    /// This document's steps on the window's undo stack, put back when it
    /// shows again: the window holds the shown document's steps only (#672).
    var systemUndoSteps = 0
    /// The screen the window's steps belong to (`attachSystemUndo`).
    var systemUndoScreen: UUID?
    var showsFind = false
    /// Try a sample's names, proposed once recognition has read the text.
    var storedNames: [SampleDocument.Name]?
    /// Identities wanted: a regular window shows them in the inspector or
    /// the right column. The user's intent, apart from how it shows
    /// (ADR 0104).
    private(set) var wantsIdentities = false
    /// The right-edge drawer of a compact window is open. Only opening
    /// Identities there opens it: a window that narrows never does (#674).
    private(set) var identitiesDrawerOpen = false
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
        snapshot = session.snapshot()
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
        select(candidate)
    }

    /// The first proposal after the one under review, wrapping around.
    func reviewNext() {
        let order = reviewOrder
        if let next = order.nextUndecided(after: order.index(of: reviewing)) {
            select(next.id)
        }
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

    /// Hide name on a selection: a proposal in the core's guessed category,
    /// with the bar open on it (ADR 0098). Undo removes the mark.
    func markSelection(_ selection: SourceRange) {
        guard !reviewWaitsForJob() else { return }
        // A selection to the line's end takes its newline; the name is the
        // words.
        let selection = Self.trimmed(selection, in: snapshot.markdown)
        guard selection.length > 0 else { return }
        guard performReview({ try session.markSelection(selection) }) else { return }
        let marked = snapshot.candidates.first { $0.range.start <= selection.start && $0.range.end >= selection.end }
            ?? ReviewOrder(snapshot).candidate(near: selection.start)
        if let marked {
            focus = nil
            select(marked.id)
            Haptics.decision()
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

    private func say(_ text: String, symbol: String = "checkmark.circle.fill") {
        notice = text
        noticeSymbol = symbol
    }

    /// `range` without the whitespace and newlines at its ends.
    static func trimmed(_ range: SourceRange, in text: String) -> SourceRange {
        let bytes = Array(text.utf8)
        var start = max(0, range.start), end = min(bytes.count, range.end)
        let space: Set<UInt8> = [0x20, 0x09, 0x0A, 0x0D]
        while start < end, space.contains(bytes[start]) { start += 1 }
        while end > start, space.contains(bytes[end - 1]) { end -= 1 }
        return SourceRange(start: start, end: end)
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
        let placement = identitiesPlacement
        if show && placement == .column && pane == .reply { pane = .text }
        wantsIdentities = show
        identitiesDrawerOpen = show && placement == .drawer
    }

    /// Shows `pane`. In the right column a pane takes Identities' place.
    func show(_ pane: Pane) {
        if identitiesPlacement == .column { wantsIdentities = false }
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
    }

    // MARK: Helpers

    func refresh() {
        applyReview(session.snapshot())
    }

    private func perform(_ change: () throws -> DocumentSnapshot) {
        do {
            let before = snapshot.undoStep
            applyReview(try change())
            if snapshot.canUndo && snapshot.undoStep != before { registerSystemUndo() }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func applyReview(_ new: DocumentSnapshot) {
        snapshot = new
        if let reviewing, new.candidate(reviewing) == nil {
            self.reviewing = nil
        }
        if new.reply == nil && pane == .reply {
            pane = .text
        }
        settleIdentities()
        onChange()
    }
}
