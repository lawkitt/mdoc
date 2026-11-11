import MdocFFI
import UIKit
import XCTest
@testable import mdoc

/// A name's long-press menu (ADR 0098): its header and rows carry the
/// category's colour as far as a system menu lets them, and an alias is
/// never broken.
@MainActor
final class CandidateMenuTests: XCTestCase {
    private func model() throws -> DocumentViewModel {
        let seeded = try Fixture.seeded()
        return try seeded.model(seeded.letter)
    }

    private static let styles = [UITraitCollection(userInterfaceStyle: .light), UITraitCollection(userInterfaceStyle: .dark)]

    /// The alias's tint is the document's for a replaced name, in light
    /// and dark mode, from the one colour table.
    func testTheAliasFillIsTheDocumentsForEachCategory() {
        for category in Category.allCases {
            for traits in Self.styles {
                let dark = traits.userInterfaceStyle == .dark
                let document = category.uiColor.resolvedColor(with: traits)
                    .withAlphaComponent(MarkdownStyler.fill(dark: dark).replaced)
                XCTAssertEqual(
                    Self.rgba(category.aliasFill.resolvedColor(with: traits)), Self.rgba(document),
                    "\(category.label), \(dark ? "dark" : "light")")
            }
        }
    }

    /// The header says the name, an arrow and the alias; only the alias
    /// sits on its category's tint, in the text's own colour.
    func testTheHeaderTintsOnlyTheAlias() throws {
        let model = try model()
        for candidate in model.snapshot.candidates {
            let header = CandidateMenu.header(for: candidate)
            XCTAssertEqual(header.string, "\(candidate.original) → \(candidate.alias)")
            let alias = (header.string as NSString).range(of: candidate.alias, options: .backwards)
            var tinted = NSRange()
            let fill = header.attribute(.backgroundColor, at: alias.location, effectiveRange: &tinted) as? UIColor
            XCTAssertEqual(tinted, alias, "the alias and nothing else")
            for traits in Self.styles {
                XCTAssertEqual(
                    fill.map { Self.rgba($0.resolvedColor(with: traits)) },
                    Self.rgba(candidate.category.aliasFill.resolvedColor(with: traits)))
            }
            XCTAssertNil(header.attribute(.backgroundColor, at: 0, effectiveRange: nil))
            XCTAssertEqual(header.attribute(.foregroundColor, at: alias.location, effectiveRange: nil) as? UIColor, .label)
            let font = try XCTUnwrap(header.attribute(.font, at: alias.location, effectiveRange: nil) as? UIFont)
            XCTAssertTrue(font.fontDescriptor.symbolicTraits.contains(.traitMonoSpace))
        }
    }

    /// A menu draws its titles in one colour, so each category's row has
    /// its symbol in the category's colour; the menu has no title of its
    /// own, the header being its preview.
    func testChangeCategoryRowsCarryTheirCategorysColour() throws {
        let model = try model()
        let candidate = try XCTUnwrap(model.snapshot.candidates.first)
        let menu = try XCTUnwrap(CandidateMenu.make(for: candidate.id, model: model) { _ in })
        XCTAssertEqual(menu.title, "")
        let change = try XCTUnwrap(menu.children.compactMap { $0 as? UIMenu }.first { $0.title == "Change category" })
        let others = Category.allCases.filter { $0 != candidate.category }
        XCTAssertEqual(change.children.map(\.title), others.map(\.label))
        for (row, category) in zip(change.children, others) {
            let image = try XCTUnwrap(row.image, category.label)
            XCTAssertEqual(image.renderingMode, .alwaysOriginal, "a template image would take the menu's colour")
            XCTAssertEqual(image.pngData(), category.menuIcon?.pngData(), category.label)
        }
        XCTAssertNotEqual(Category.person.menuIcon?.pngData(), UIImage(systemName: Category.person.symbol)?.pngData())
        XCTAssertNotNil(CandidateMenu.configuration(for: candidate.id, model: model) { _ in })
    }

    /// Same as… rows: the target's category colour on the icon, and its
    /// alias with no place to break; VoiceOver hears the alias as written.
    func testSameAsRowsKeepTheAliasWhole() throws {
        let (core, storage) = try Fixture.core()
        let session = try core.library.importDocument(
            .text("Maya Bennett and Daniel Okafor signed for Riverside Holdings Ltd."), title: "Note",
            into: .newCase(name: "Bennett"))
        _ = try session.recognize(observer: TestObserver())
        _ = try session.propose([
            SampleDocument.Name(wording: "Maya Bennett", category: .person),
            SampleDocument.Name(wording: "Daniel Okafor", category: .person),
            SampleDocument.Name(wording: "Riverside Holdings Ltd", category: .organization),
        ])
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        let candidate = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Maya Bennett" })
        let targets = model.sameAsTargets(for: candidate).all
        XCTAssertEqual(targets.count, 2)
        let menu = try XCTUnwrap(CandidateMenu.make(for: candidate.id, model: model) { _ in })
        let sameAs = try XCTUnwrap(menu.children.compactMap { $0 as? UIMenu }.first { $0.title == "Same as…" })
        func rows(_ menu: UIMenu) -> [UIAction] {
            menu.children.flatMap { ($0 as? UIMenu).map(rows) ?? [$0 as? UIAction].compactMap { $0 } }
        }
        let found = rows(sameAs)
        XCTAssertEqual(found.count, targets.count)
        for row in found {
            let target = try XCTUnwrap(targets.first { row.title.hasPrefix($0.primaryWording + " · ") }, row.title)
            XCTAssertTrue(row.title.hasSuffix(target.alias.unbrokenAlias))
            XCTAssertEqual(row.accessibilityLabel, "\(target.primaryWording), \(target.alias)")
            XCTAssertEqual(row.image?.pngData(), target.category.menuIcon?.pngData())
        }
    }

    /// A word joiner between every two characters, and nothing else added;
    /// a long alias is left as it is.
    func testAnUnbrokenAliasHasNoBreakPoint() {
        let alias = "PERSON_6"
        XCTAssertEqual(alias.unbrokenAlias.replacingOccurrences(of: "\u{2060}", with: ""), alias)
        XCTAssertEqual(alias.unbrokenAlias.unicodeScalars.filter { $0 == "\u{2060}" }.count, alias.count - 1)
        XCTAssertEqual("".unbrokenAlias, "")
        let long = String(repeating: "A", count: String.unbrokenAliasLimit + 1)
        XCTAssertEqual(long.unbrokenAlias, long, "a long alias wraps rather than lose its end")
    }

    private static func rgba(_ colour: UIColor) -> [Int] {
        var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
        colour.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return [red, green, blue, alpha].map { Int(($0 * 255).rounded()) }
    }
}
