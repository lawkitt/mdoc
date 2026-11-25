import MdocFFI
import UIKit

/// A selection copied by hand (#1209): what its menu offers and its copy.
/// The core decides; every copy is checked again just before it is made,
/// and copies marked unless that check finds it clean. Nothing says so
/// after a copy.
extension DocumentViewModel {
    /// The core's check of `range` of the text as it is now, off the main
    /// thread; nil when it failed or the text changed meanwhile.
    func checkSelection(_ range: SourceRange) async -> SelectionCheck? {
        let session = session
        let revision = snapshot.textRevision
        let check = await Task.detached(priority: .userInitiated) { try? session.checkSelection(range) }.value
        guard snapshot.textRevision == revision else { return nil }
        return check
    }

    /// Puts a hand copy on the clipboard: the text, and whether it is
    /// marked. Tests pass their own.
    typealias CopyWriter = @MainActor (String, Bool) -> Void

    /// The core's check of `range`, made for a copy of `text`: nil unless
    /// the core's text there is `text` both before and after the check, so
    /// the answer is about the bytes copied (edit mode's typing moves the
    /// text on without a new snapshot here).
    private func checkCopy(_ text: String, at range: SourceRange) async -> SelectionCheck? {
        let session = session
        return await Task.detached(priority: .userInitiated) {
            let holds = { SelectionCopy.text(range, in: session.snapshot().markdown) == text }
            guard holds(), let check = try? session.checkSelection(range), holds() else { return nil }
            return check
        }.value
    }

    /// Copy, Copy with names, ⌘C and VoiceOver's Copy: `text`, the
    /// selection as written, at `range` of the text. It is marked unless a
    /// check made now finds `range` clean; nil `range` (a selection not of
    /// the text the core holds) copies marked with no check. A newer copy,
    /// or anything else put on the clipboard meanwhile, wins: this one is
    /// then not written.
    @discardableResult
    func copySelection(
        _ text: String, at range: SourceRange?, write: @escaping CopyWriter = { SelectionCopy.write($0, marked: $1) }
    ) -> Task<Void, Never> {
        let request = SelectionCopy.Request()
        return Task { [weak self] in
            let check: SelectionCheck? = if let range, let self { await self.checkCopy(text, at: range) } else { nil }
            guard request.isLatest else { return }
            write(text, SelectionCopy.marked(check))
        }
    }

    /// Replace N and copy: the proposals in `range` replaced as the
    /// review's Replace does, one undo step, then the replaced text copied,
    /// widened so each alias is whole, and marked unless it is clean now.
    /// `revision` is the text the selection was made in: a change since
    /// replaces nothing. Returns the copy's work, nil when nothing copied.
    @discardableResult
    func replaceAndCopy(
        _ range: SourceRange, in revision: UInt64?, write: @escaping CopyWriter = { SelectionCopy.write($0, marked: $1) }
    ) -> Task<Void, Never>? {
        guard !reviewWaitsForJob() else { return nil }
        guard revision == snapshot.textRevision else {
            errorMessage = String(localized: "The document changed. Select the text again.")
            return nil
        }
        var replacement: SelectionReplacement?
        guard perform({
            let replaced = try session.replaceSelection(range)
            replacement = replaced
            return replaced.snapshot
        }), let replacement, let text = SelectionCopy.text(replacement.range, in: replacement.snapshot.markdown)
        else { return nil }
        return copySelection(text, at: replacement.range, write: write)
    }
}

/// What the text view's selection menu reaches (#1209).
struct SelectionCopyActions {
    /// The core's check of a selection, off the main thread; nil when it
    /// failed.
    var check: @MainActor (SourceRange) async -> SelectionCheck?
    /// Copies the text as written, at a range of the text (nil: no check).
    var copy: @MainActor (String, SourceRange?) -> Void
    /// Replace N and copy, for a selection made in a text revision.
    var replaceAndCopy: @MainActor (SourceRange, UInt64?) -> Void
}

extension SelectionCopyActions {
    @MainActor
    init(_ model: DocumentViewModel) {
        self.init(
            check: { [weak model] in await model?.checkSelection($0) },
            copy: { [weak model] text, range in
                if let model { model.copySelection(text, at: range) } else { SelectionCopy.write(text, marked: true) }
            },
            replaceAndCopy: { [weak model] in model?.replaceAndCopy($0, in: $1) })
    }
}
