import Foundation
import MdocFFI

/// What slice 6 of ADR 0109 adds to a document: a new document opening in
/// edit mode, Find names again over edited text, and the sample restored.
extension DocumentViewModel {
    /// New document: edit mode, keyboard up, once its empty text is read;
    /// at once when it is read already.
    func editsOnceRead() {
        if snapshot.phase == nil {
            editsWhenRead = true
        } else {
            enterEditMode()
        }
    }

    // MARK: Find names again (ADR 0109)

    /// Find names again shows in the document's menu once names were looked
    /// for, where Find names would run: never below the detector floor,
    /// which finds no names (ADR 0088). An unscanned document has Find
    /// names itself.
    var offersFindNamesAgain: Bool {
        findsNames && (snapshot.phase == .reviewing || snapshot.phase == .ready)
    }

    /// Why Find names again waits: it runs only outside edit mode, and not
    /// while a job runs for the document.
    var findNamesAgainBlockedReason: String? {
        if isEditing { return String(localized: "Leave edit mode to find names") }
        return runningJob == nil ? nil : String(localized: "Another job is running. Try again when it finishes.")
    }

    /// Find names again: recognition over the text as it is now, the same
    /// long job as the first Find names, so names in typed text are
    /// proposed. Decisions already made stay. Edit waits while it runs.
    func findNamesAgain() async {
        if let reason = findNamesAgainBlockedReason {
            say(reason, symbol: "hourglass")
            return
        }
        if isReviewing { endReview() }
        await findNames()
    }

    // MARK: The sample (ADR 0109)

    /// Try a sample again on this, the sample: its text as it came, as one
    /// edit (one Undo brings the edits back), with its names proposed again.
    func restore(_ sample: SampleDocument) {
        if isEditing { doneEditing() }
        if isReviewing { endReview() }
        let whole = SourceRange(start: 0, end: session.snapshot().markdown.utf8.count)
        do {
            let typed = try session.type(whole, sample.text)
            session.endTyping()
            if typed.newStep { registerSystemUndo(label: String(localized: "Typing")) }
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        // The names, as Try a sample proposes them; the snapshot shows both.
        proposeStored(sample.names)
        onChange()
    }
}
