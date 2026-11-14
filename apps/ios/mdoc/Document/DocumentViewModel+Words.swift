import Foundation
import MdocFFI

/// Fix the words in progress (ADR 0114): the name, the draft the text shows
/// meanwhile, and the words its handles hold now.
struct WordsFix: Equatable {
    let candidate: CandidateID
    let draft: WordsDraft
    /// The handles' words, in the draft's source bytes.
    var selection: SourceRange
    /// What Hide «…» would take for `selection`, or the core's reason it
    /// can't.
    var target: Result<WordsTarget, CoreError>
}

/// A name's words (ADR 0114): Hide name's callout for a selection outside
/// any name, and Fix the words for a found one. The core snaps, trims and
/// decides; the model asks it and keeps the fix's view state.
extension DocumentViewModel {
    // MARK: Hide name

    /// Hide name's callout for `selection`: what it hides and Hide as's
    /// categories, the guess first; nil when the core finds nothing to
    /// hide there in its own terms (the menu then offers plain Hide name).
    func hideTarget(for selection: SourceRange) -> WordsTarget? {
        try? session.hideTarget(selection)
    }

    /// Hide name on a selection: the words the callout named, as a proposal
    /// in `category` (Hide as) or the core's guess, with the bar open on it
    /// (ADR 0098). Undo removes the mark.
    func markSelection(_ selection: SourceRange, as category: Category? = nil) {
        guard !reviewWaitsForJob() else { return }
        let marked = hideTarget(for: selection).map { SourceRange($0.range) } ?? selection
        guard marked.length > 0 else { return }
        guard perform({ try session.markSelection(marked, as: category) }) else { return }
        if updateReview({ $0.selectMarked(marked) }) { Haptics.decision() }
    }

    // MARK: Fix the words

    /// What the text shows: the draft while a name's words are fixed.
    var shownSnapshot: DocumentSnapshot { wordsFix?.draft.snapshot ?? snapshot }

    /// Fix the words on `id`, from its menu or the card: the text shows its
    /// draft (a replaced name's original back) with handles on its words.
    func beginFixWords(_ id: CandidateID) {
        guard !reviewWaitsForJob(), !isEditing else { return }
        do {
            let draft = try session.fixWordsDraft(id)
            wordsFix = WordsFix(
                candidate: id, draft: draft, selection: draft.wording,
                target: target(of: id, words: draft.wording))
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// The handles moved: Hide «…» follows them.
    func fixSelectionChanged(_ words: SourceRange) {
        guard var fix = wordsFix, fix.selection != words else { return }
        fix.selection = words
        fix.target = target(of: fix.candidate, words: words)
        wordsFix = fix
    }

    /// Hide «…»: the name takes the words its handles hold, everywhere the
    /// old wording fits, as one step; the bar moves onto it, with Undo.
    func confirmFixWords() {
        guard let fix = wordsFix else { return }
        wordsFix = nil
        guard case .success(let target) = fix.target else { return }
        let before = snapshot.undoStep
        guard perform({ try session.fixWords(fix.candidate, words: SourceRange(target.range)) }),
              snapshot.undoStep != before else { return }
        let step = snapshot.undoStep
        updateReview { $0.follow(step) }
        Haptics.decision()
        toast = undoToast(String(localized: "Words fixed: \(target.wording)"))
    }

    /// Cancel, a tap away from the handles, or another change: the text
    /// shows the document again, unchanged.
    func cancelFixWords() {
        wordsFix = nil
    }

    private func target(of id: CandidateID, words: SourceRange) -> Result<WordsTarget, CoreError> {
        do {
            return .success(try session.fixWordsTarget(id, words: words))
        } catch let error as CoreError {
            return .failure(error)
        } catch {
            return .failure(.other(error.localizedDescription))
        }
    }
}
