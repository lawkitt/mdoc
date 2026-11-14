import MdocFFI
import XCTest
@testable import mdoc

/// A name's words (ADR 0114): Hide name's callout for a selection outside
/// any name, and Fix the words on a found one, through the real core.
@MainActor
final class NameWordsTests: XCTestCase {
    private func model() throws -> DocumentViewModel {
        let seeded = try Fixture.seeded()
        return try seeded.model(seeded.letter)
    }

    /// The UTF-8 range of the `nth` `words` in `text`.
    private func range(of words: String, in text: String, nth: Int = 0) throws -> SourceRange {
        var from = text.startIndex
        var found: Range<String.Index>?
        for _ in 0...nth {
            found = try XCTUnwrap(text.range(of: words, range: from..<text.endIndex))
            from = found!.upperBound
        }
        let match = try XCTUnwrap(found)
        let start = text.utf8.distance(from: text.startIndex, to: match.lowerBound)
        return SourceRange(start: start, end: start + words.utf8.count)
    }

    // MARK: Hide name

    /// The callout names whole words without the punctuation at the ends,
    /// and Hide as offers every category, the core's guess first.
    func testHideNameNamesTheWholeWordsAndOffersTheGuessFirst() throws {
        let model = try model()
        let text = model.snapshot.markdown
        // "staircas" and the space before it: the word counts whole.
        let selection = try range(of: " staircas", in: text)
        let target = try XCTUnwrap(model.hideTarget(for: selection))
        XCTAssertEqual(target.wording, "staircase")
        XCTAssertEqual(Set(target.categories), Set(Category.allCases))
        XCTAssertEqual(target.categories.count, Category.allCases.count)
        // A proposed wording keeps its own category, alone.
        let proposed = try range(of: "Priya Nandakumar", in: text)
        XCTAssertEqual(model.hideTarget(for: proposed)?.categories, [.person])
        // Nothing but punctuation: nothing to hide.
        XCTAssertNil(model.hideTarget(for: try range(of: "**", in: text)))
    }

    /// A selection to the line's end takes its newline; the name is the
    /// words.
    func testAMarkedSelectionLosesItsLineEndAndPunctuation() throws {
        let model = try model()
        let text = model.snapshot.markdown
        let selection = try range(of: "on 3 July 2026 ", in: text)
        model.markSelection(selection)
        XCTAssertEqual(model.reviewedCandidate?.original, "on 3 July 2026")
        let title = try range(of: "Letter before claim\n", in: text)
        model.markSelection(title, as: .other)
        XCTAssertEqual(model.reviewedCandidate?.original, "Letter before claim")
    }

    /// Hide as: the category picked, not the guess.
    func testHideAsMarksInTheCategoryPicked() throws {
        let model = try model()
        let selection = try range(of: "staircase", in: model.snapshot.markdown)
        model.markSelection(selection, as: .organization)
        XCTAssertEqual(model.reviewedCandidate?.original, "staircase")
        XCTAssertEqual(model.reviewedCandidate?.category, .organization)
        XCTAssertTrue(model.canUndo)
    }

    // MARK: Fix the words

    /// Narrowed words take the old wording's place at every repeat, keep
    /// the alias, and go back in one Undo.
    func testFixTheWordsNarrowsEveryRepeatAndUndoTakesItBack() throws {
        let model = try model()
        let maya = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Maya Ellison" })
        let alias = maya.alias
        let repeats = model.snapshot.candidates.filter { $0.original == "Maya Ellison" }.count
        XCTAssertGreaterThan(repeats, 1)
        model.select(maya.id)
        model.perform(.fixWords)
        let fix = try XCTUnwrap(model.wordsFix)
        let draft = fix.draft.snapshot.markdown
        XCTAssertEqual(draft, model.snapshot.markdown, "a proposal's draft is the text")
        // The handles move onto the surname.
        let surname = SourceRange(start: fix.draft.wording.end - "Ellison".utf8.count, end: fix.draft.wording.end)
        model.fixSelectionChanged(surname)
        guard case .success(let target) = model.wordsFix?.target else { return XCTFail("Ellison can be hidden") }
        XCTAssertEqual(target.wording, "Ellison")
        XCTAssertEqual(FixWordsBar.hideTitle(for: try XCTUnwrap(model.wordsFix)), "Hide «Ellison»")
        model.confirmFixWords()
        XCTAssertNil(model.wordsFix)
        XCTAssertFalse(model.snapshot.candidates.contains { $0.original == "Maya Ellison" })
        let fixed = model.snapshot.candidates.filter { $0.original == "Ellison" }
        XCTAssertEqual(fixed.count, repeats)
        XCTAssertEqual(Set(fixed.map(\.alias)), [alias])
        XCTAssertEqual(model.reviewedCandidate?.original, "Ellison", "the bar is on the fixed name")
        XCTAssertEqual(model.snapshot.undoStep?.kind, .fixWords)
        XCTAssertNotNil(model.toast)
        model.undo()
        XCTAssertEqual(model.snapshot.candidates.filter { $0.original == "Maya Ellison" }.count, repeats)
    }

    /// A replaced name shows its original while its words are fixed, and
    /// stays replaced under its alias after.
    func testAReplacedNameShowsItsOriginalWhileFixed() throws {
        let model = try model()
        let tom = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Priya Nandakumar" })
        model.decide(tom.id, .replace, scope: .all)
        let replaced = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Priya Nandakumar" })
        XCTAssertEqual(replaced.state, .replaced)
        model.beginFixWords(replaced.id)
        let fix = try XCTUnwrap(model.wordsFix)
        XCTAssertTrue(model.shownSnapshot.markdown.contains("colleague Priya Nandakumar on"))
        XCTAssertFalse(model.snapshot.markdown.contains("colleague Priya Nandakumar on"))
        XCTAssertEqual(model.shownSnapshot.candidate(replaced.id)?.state, .proposed)
        // Wider: "colleague Priya Nandakumar".
        let wider = try range(of: "colleague Priya Nandakumar", in: fix.draft.snapshot.markdown)
        XCTAssertLessThanOrEqual(fix.draft.bounds.start, wider.start)
        model.fixSelectionChanged(wider)
        model.confirmFixWords()
        XCTAssertTrue(model.snapshot.markdown.contains("your \(replaced.alias) on"))
        XCTAssertEqual(model.shownSnapshot, model.snapshot)
    }

    /// Cancel, or the same words, leave the document as it was.
    func testCancelOrTheSameWordsChangeNothing() throws {
        let model = try model()
        let before = model.snapshot
        let name = try XCTUnwrap(before.candidates.first { $0.state == .proposed })
        model.beginFixWords(name.id)
        XCTAssertNotNil(model.wordsFix)
        model.cancelFixWords()
        XCTAssertNil(model.wordsFix)
        model.beginFixWords(name.id)
        model.confirmFixWords()
        XCTAssertEqual(model.snapshot.markdown, before.markdown)
        XCTAssertEqual(model.snapshot.canUndo, before.canUndo)
        XCTAssertNil(model.toast)
        // Only spaces: the bar says why and Hide is off.
        model.beginFixWords(name.id)
        let space = SourceRange(start: name.range.end, end: name.range.end + 1)
        model.fixSelectionChanged(space)
        guard case .failure = model.wordsFix?.target else { return XCTFail("a space can't be hidden") }
    }

    /// Fix the words is in every name's long-press menu, last, and the
    /// card's chips.
    func testFixTheWordsIsInEveryNamesMenu() throws {
        let model = try model()
        for candidate in model.snapshot.candidates {
            XCTAssertEqual(model.actions(for: candidate).last, .fixWords, candidate.original)
        }
        let menu = try XCTUnwrap(CandidateMenu.make(for: model.snapshot.candidates[0].id, model: model) { _ in })
        XCTAssertEqual(menu.children.last?.title, "Fix the words")
    }

    /// The menu's quoted words are cut at a word's end past 24 characters.
    func testAMenuQuotesALongNameCutAtAWord() {
        XCTAssertEqual("Тагиев Тебриз".truncatedForMenu, "Тагиев Тебриз")
        XCTAssertEqual("Тагиев Тебриз Фарадж оглы".truncatedForMenu, "Тагиев Тебриз Фарадж…")
    }
}
