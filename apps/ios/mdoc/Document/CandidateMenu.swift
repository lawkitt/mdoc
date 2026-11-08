import MdocFFI
import UIKit

/// A long press on a name (ADR 0098): the view model's actions for it as a
/// `UIMenu`, under a header with the name and its alias. Each choice puts
/// the bar on the name, then acts. Building it changes nothing; the model
/// hears the long press through `menuOpened(on:)`.
@MainActor
enum CandidateMenu {
    /// `openSameAsList` opens the searchable picker for a long Same as… list.
    static func make(
        for id: CandidateID, model: DocumentViewModel, openSameAsList: @escaping (CandidateID) -> Void
    ) -> UIMenu? {
        guard let candidate = model.snapshot.candidate(id) else { return nil }
        let children = model.actions(for: candidate).map { element(for: $0, candidate: candidate, model: model, openSameAsList: openSameAsList) }
        return UIMenu(title: "\(candidate.original) → \(candidate.alias)", children: children)
    }

    private static func element(
        for action: CandidateAction, candidate: Candidate, model: DocumentViewModel,
        openSameAsList: @escaping (CandidateID) -> Void
    ) -> UIMenuElement {
        let id = candidate.id
        switch action {
        case .sameAs:
            return sameAs(candidate, model: model, openSameAsList: openSameAsList)
        case .changeCategory:
            return UIMenu(
                title: String(localized: "Change category"), image: UIImage(systemName: "tag"),
                children: Category.allCases.filter { $0 != candidate.category }.map { category in
                    item(category.label, category.symbol) { model.setCategory(category, fromMenuOn: id) }
                })
        case .replace, .keep, .replaceThisOne, .showOriginal, .keepInstead, .replaceInstead, .next:
            let (title, image) = label(of: action)
            return item(title, image) { model.perform(action, fromMenuOn: id) }
        }
    }

    /// The same order as the card's picker; past eight, its searchable sheet.
    private static func sameAs(
        _ candidate: Candidate, model: DocumentViewModel, openSameAsList: @escaping (CandidateID) -> Void
    ) -> UIMenuElement {
        let id = candidate.id
        let found = model.sameAsTargets(for: candidate)
        if found.all.count > DocumentViewModel.sameAsListLimit {
            return item(String(localized: "Same as…"), "link") {
                model.openSameAsList(fromMenuOn: id)
                openSameAsList(id)
            }
        }
        func targets(_ identities: [Identity]) -> [UIMenuElement] {
            identities.map { target in
                item("\(target.primaryWording) · \(target.alias)", target.category.symbol) {
                    model.sameAs(target.id, fromMenuOn: id)
                }
            }
        }
        return UIMenu(title: String(localized: "Same as…"), image: UIImage(systemName: "link"), children: [
            UIMenu(title: String(localized: "Looks like"), options: .displayInline, children: targets(found.hinted)),
            UIMenu(options: .displayInline, children: targets(found.sameCategory)),
            UIMenu(title: String(localized: "Other categories"), children: targets(found.otherCategories)),
        ].filter { !$0.children.isEmpty })
    }

    private static func label(of action: CandidateAction) -> (String, String) {
        switch action {
        case .replace: (String(localized: "Replace"), "eye.slash")
        case .keep: (String(localized: "Keep"), "eye")
        case .replaceThisOne: (String(localized: "Just this one"), "1.circle")
        case .showOriginal: (String(localized: "Show original"), "text.quote")
        case .keepInstead: (String(localized: "Keep instead"), "eye")
        case .replaceInstead: (String(localized: "Replace instead"), "eye.slash")
        case .next: (String(localized: "Next"), "arrow.right")
        case .sameAs: (String(localized: "Same as…"), "link")
        case .changeCategory: (String(localized: "Change category"), "tag")
        }
    }

    private static func item(_ title: String, _ image: String, _ run: @escaping () -> Void) -> UIAction {
        UIAction(title: title, image: UIImage(systemName: image)) { _ in run() }
    }
}
