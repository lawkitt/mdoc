import Foundation
import UIKit

/// Copy for AI and Share refusals in plain words (#1209 decisions 13 and
/// 14): every refusal names its reason and carries the one step that
/// resolves it, before the tap where the document can tell, and as an alert
/// where only the tap finds it. The bar and the title menu read the same
/// refusal, so they agree, below the floor too.
extension DocumentViewModel {
    /// Why Copy for AI and Share wait now; nil when they go. The review's
    /// state first, then what the Vault says of the document as saved:
    /// changed since its review, a known name written out, excluded from AI.
    var handOffRefusal: HandOffRefusal? {
        switch snapshot.phase {
        case .ready?:
            break
        case .reviewing?:
            return .namesToDecide(names: identityCounts.toReview, proposals: snapshot.pendingCount)
        case .unscanned?:
            if findsNames { return .notScanned }
            // Below the floor names are decided by hand: with none left the
            // document goes, as the bar's Share says (#1091 Q4).
            if snapshot.pendingCount > 0 {
                return .namesToDecide(names: identityCounts.toReview, proposals: snapshot.pendingCount)
            }
        case nil:
            return .notRead
        }
        // A record not saved Ready while the session is done is one the
        // tap's save brings up to date (a phone fallen below the floor
        // since); where it can't, the tap says why.
        guard let saved = savedReadiness, saved != .notReviewed else { return nil }
        return HandOffRefusal(saved)
    }

    /// Under Copy for AI and Share in the document's menu (ADR 0115).
    var handOffBlockedReason: String? { handOffRefusal?.reason }

    /// The step that resolves `refusal` on this phone.
    func step(for refusal: HandOffRefusal) -> HandOffRefusal.Step? {
        refusal.step(findsNames: findsNames)
    }

    /// Asks the Vault, off the main thread, what it says of the document as
    /// saved now. After each save, a scan and on refresh: a name another
    /// document added to the Project, or a Keep of one it knows, shows here.
    @discardableResult
    func checkSaved() -> Task<Void, Never> {
        let session = session
        let revision = (snapshot.textRevision, snapshot.undoStep, snapshot.phase)
        savedCheck?.cancel()
        let check = Task { [weak self] in
            let readiness = await Task.detached(priority: .userInitiated) { session.savedReadiness() }.value
            guard let self, !Task.isCancelled,
                  (snapshot.textRevision, snapshot.undoStep, snapshot.phase) == revision else { return }
            savedReadiness = readiness
        }
        savedCheck = check
        return check
    }

    /// A hand-off that failed: a refusal shows with its step; anything else
    /// as before.
    func refused(_ error: any Error) {
        if case CoreError.handOffRefused(let why) = error {
            // The tap scanned an edit Done had not scanned yet: a new name
            // it proposed shows on the bar, and Replace N and copy counts it.
            if !isEditing { applyReview(session.snapshot()) }
            refusal = why
            // What the tap found, the bar and the menu show too.
            checkSaved()
        } else {
            errorMessage = error.localizedDescription
        }
    }

    /// The refusal's step: Replace N and copy accepts the proposals as the
    /// review's Replace all does, one undoable step, then copies; Review
    /// opens the bar on the names left; Find names looks for names again.
    func take(_ step: HandOffRefusal.Step) {
        refusal = nil
        switch step {
        case .replaceAndCopy:
            replaceAllPending()
            guard snapshot.pendingCount == 0 else { return }
            copyForAI { UIPasteboard.general.string = $0 }
        case .review:
            if snapshot.pendingCount > 0 {
                beginReview()
            } else if let kept = snapshot.candidates.first(where: { $0.state == .kept }) {
                // A known name written out: a mention kept as written.
                open(kept.id)
            } else if perform(undoable: false, { try session.lineUpWithProject() }) {
                // Opened again, its history starts afresh: so does the window's.
                systemUndo?.removeAllActions(withTarget: self)
                systemUndoSteps = 0
                // A name another document added to the Project since this
                // review began is proposed now.
                if snapshot.pendingCount > 0 { beginReview() } else { showIdentities(true) }
            } else {
                showIdentities(true)
            }
        case .findNames:
            Task { await findNamesAgain() }
        }
    }
}
