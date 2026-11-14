import Foundation
import MdocFFI

/// What can be done to a name, by its state (ADR 0098, 0101). The long
/// press's menu, the card's buttons and the hardware keys all take their
/// actions from here, so they can't disagree (#678).
enum CandidateAction: Hashable {
    case replace
    case keep
    /// Replace this mention only, from the long press.
    case replaceThisOne
    case keepInstead
    case replaceInstead
    /// The next undecided name, or Done.
    case next
    /// Same as…: a submenu of identities, or the searchable picker past
    /// eight (`DocumentViewModel.sameAsListLimit`).
    case sameAs
    /// Change category: a submenu of the other categories.
    case changeCategory
    /// Fix the words (ADR 0114): handles on the name in the text.
    case fixWords
}

/// A hardware key the review bar answers (ADR 0098).
enum ReviewKey: Hashable {
    /// Return: the card's replace, or Next once replaced.
    case `return`
    /// K: the card's keep.
    case keep
}

extension CandidateAction {
    /// The decisions a name in `state` offers: Replace and Keep while
    /// proposed, else the other choice ("instead").
    static func decisions(for state: MentionState) -> [CandidateAction] {
        switch state {
        case .proposed: [.replace, .keep]
        case .replaced: [.keepInstead]
        case .kept: [.replaceInstead]
        }
    }

    /// The decision's outcome, when it is one.
    var decision: Decision? {
        switch self {
        case .replace, .replaceThisOne, .replaceInstead: .replace
        case .keep, .keepInstead: .keep
        case .next, .sameAs, .changeCategory, .fixWords: nil
        }
    }
}

extension DocumentViewModel {
    /// Past this many identities Same as… opens a searchable list instead of
    /// a menu (ADR 0098).
    static let sameAsListLimit = 8

    /// The long press's actions on a name, in menu order: its decisions
    /// (with Just this one while proposed), Same as… when there is another
    /// identity and the name is not kept, Change category, then Fix the
    /// words (ADR 0114). A replaced name's original wording is the menu's
    /// header (`CandidateMenu`).
    func actions(for candidate: Candidate) -> [CandidateAction] {
        var actions = CandidateAction.decisions(for: candidate.state)
        if candidate.state == .proposed { actions.append(.replaceThisOne) }
        if candidate.state != .kept && !sameAsTargets(for: candidate).all.isEmpty {
            actions.append(.sameAs)
        }
        actions.append(.changeCategory)
        actions.append(.fixWords)
        return actions
    }

    /// The card's two buttons beside Undo: Keep and Replace (filled) while
    /// proposed, else the other choice and Next (filled).
    func cardActions(for candidate: Candidate) -> (other: CandidateAction, filled: CandidateAction) {
        let decisions = CandidateAction.decisions(for: candidate.state)
        return decisions.count > 1 ? (decisions[1], decisions[0]) : (decisions[0], .next)
    }

    /// What a key does on the card: Return replaces, or goes Next once
    /// replaced; K keeps, and does nothing on a kept name.
    func keyAction(_ key: ReviewKey, for candidate: Candidate) -> CandidateAction? {
        let decisions = CandidateAction.decisions(for: candidate.state)
        switch key {
        case .return: return decisions.first { $0.decision == .replace } ?? .next
        case .keep: return decisions.first { $0.decision == .keep }
        }
    }

    /// Runs `action` on the card under review, in the bar's scope.
    func perform(_ action: CandidateAction) {
        switch action {
        case .replace, .keep, .keepInstead, .replaceInstead:
            if let decision = action.decision { decideCurrent(decision) }
        case .replaceThisOne:
            setScope(.this)
            decideCurrent(.replace)
        case .next:
            goToNextUndecided()
        case .fixWords:
            if let id = reviewing { beginFixWords(id) }
        case .sameAs, .changeCategory:
            // Submenus: their choices run `sameAs` and `setCategory`.
            break
        }
    }

    /// A long-press menu's choice on `id`: the bar goes onto it in scope
    /// All (This one for Just this one) and then acts, as the card would.
    func perform(_ action: CandidateAction, fromMenuOn id: CandidateID) {
        prepareMenuChoice(on: id)
        perform(action)
    }

    /// The menu's Same as… target.
    func sameAs(_ target: IdentityID, fromMenuOn id: CandidateID) {
        prepareMenuChoice(on: id)
        sameAs(target)
    }

    /// The menu's other category.
    func setCategory(_ category: Category, fromMenuOn id: CandidateID) {
        prepareMenuChoice(on: id)
        setCategory(category)
    }

    /// The menu's Same as… past eight identities: the bar onto the name,
    /// before the picker opens.
    func openSameAsList(fromMenuOn id: CandidateID) {
        prepareMenuChoice(on: id)
    }

    /// The long press opened a name's menu: its hint is no longer needed.
    func menuOpened(on id: CandidateID) {
        learn(.longPress)
    }

    private func prepareMenuChoice(on id: CandidateID) {
        updateReview {
            $0.select(id)
            $0.setScope(.all)
        }
    }
}
