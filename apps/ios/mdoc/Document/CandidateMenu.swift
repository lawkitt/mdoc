import MdocFFI
import UIKit

/// A long press on a name (ADR 0098): the view model's actions for it as a
/// `UIMenu`, under a header with the name and its alias on its category's
/// tint, as the document draws it. Each choice puts the bar on the name,
/// then acts. Building it changes nothing; the model hears the long press
/// through `menuOpened(on:)`.
///
/// A system menu draws its titles in one colour and font, so a row cannot
/// tint its alias: its icon takes the category's colour instead
/// (`Category.menuIcon`), and the alias is kept in one piece.
@MainActor
enum CandidateMenu {
    /// The menu and its header. `openSameAsList` opens the searchable
    /// picker for a long Same as… list.
    static func configuration(
        for id: CandidateID, model: DocumentViewModel, openSameAsList: @escaping (CandidateID) -> Void
    ) -> UITextItem.MenuConfiguration? {
        guard let candidate = model.snapshot.candidate(id),
              let menu = make(for: id, model: model, openSameAsList: openSameAsList) else { return nil }
        return UITextItem.MenuConfiguration(preview: .view(MenuHeader(candidate)), menu: menu)
    }

    /// The menu alone: its header is the configuration's preview, which
    /// can draw the alias's tint; a menu's title cannot.
    static func make(
        for id: CandidateID, model: DocumentViewModel, openSameAsList: @escaping (CandidateID) -> Void
    ) -> UIMenu? {
        guard let candidate = model.snapshot.candidate(id) else { return nil }
        let children = model.actions(for: candidate).map { element(for: $0, candidate: candidate, model: model, openSameAsList: openSameAsList) }
        return UIMenu(children: children)
    }

    /// The header's words: the name, an arrow and the alias, monospaced on
    /// its category's tint as a replaced name is in the document.
    static func header(for candidate: Candidate) -> NSAttributedString {
        let body = UIFont.preferredFont(forTextStyle: .subheadline)
        let alias = UIFont.monospacedSystemFont(ofSize: body.pointSize, weight: .medium)
        let text = NSMutableAttributedString(
            string: "\(candidate.original) → ", attributes: [.font: body, .foregroundColor: UIColor.label])
        text.append(NSAttributedString(string: candidate.alias, attributes: [
            .font: alias, .foregroundColor: UIColor.label, .backgroundColor: candidate.category.aliasFill,
        ]))
        return text
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
                    UIAction(title: category.label, image: category.menuIcon) { _ in
                        model.setCategory(category, fromMenuOn: id)
                    }
                })
        case .replace, .keep, .replaceThisOne, .keepInstead, .replaceInstead, .next:
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
                let action = UIAction(
                    title: "\(target.primaryWording) · \(target.alias.unbrokenAlias)", image: target.category.menuIcon
                ) { _ in
                    model.sameAs(target.id, fromMenuOn: id)
                }
                action.accessibilityLabel = "\(target.primaryWording), \(target.alias)"
                return action
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

/// The long-press menu's header: `CandidateMenu.header(for:)` on a card.
/// VoiceOver hears the name and its alias.
private final class MenuHeader: UIView {
    private static let maxWidth: CGFloat = 320

    init(_ candidate: Candidate) {
        super.init(frame: .zero)
        let label = UILabel()
        label.attributedText = CandidateMenu.header(for: candidate)
        label.numberOfLines = 2
        label.lineBreakMode = .byTruncatingMiddle
        label.lineBreakStrategy = []
        label.adjustsFontForContentSizeCategory = false
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)
        backgroundColor = .secondarySystemGroupedBackground
        layer.cornerRadius = Radius.control
        layer.cornerCurve = .continuous
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.s),
            label.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.s),
            label.topAnchor.constraint(equalTo: topAnchor, constant: Space.xs),
            label.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Space.xs),
        ])
        // As wide as its words, up to a phone's menu; a long name wraps
        // onto a second line.
        let fitted = label.sizeThatFits(CGSize(width: Self.maxWidth - 2 * Space.s, height: .greatestFiniteMagnitude))
        frame.size = CGSize(width: ceil(fitted.width) + 2 * Space.s, height: ceil(fitted.height) + 2 * Space.xs)
        isAccessibilityElement = true
        accessibilityLabel = "\(candidate.original) → \(candidate.alias)"
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }
}
