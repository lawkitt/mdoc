import Foundation
import MdocFFI
import XCTest

/// ADR 0085's parity tests: the core called through the Swift bindings gives
/// the same bytes as the desktop's GUI-free path, checked against the
/// goldens `crates/mdoc-ffi/tests/parity.rs` writes in
/// `tests/fixtures/mobile-parity/`.
final class ParityTests: XCTestCase {
    /// The repository, from this file's path (the Simulator reads the Mac's files).
    static let repo = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()

    private func read(_ path: String) throws -> String {
        try String(contentsOf: Self.repo.appendingPathComponent(path), encoding: .utf8)
    }

    private struct ExportedIdentity: Decodable {
        var alias: String
        var category: String
        var originals: [String]
    }

    private static func category(_ token: String) -> MdocFFI.Category {
        switch token {
        case "PERSON": .person
        case "ORG": .organization
        case "EMAIL": .email
        case "PHONE": .phone
        case "ADDRESS": .address
        case "IDENTITY": .identity
        case "TAX": .tax
        case "BANK": .bank
        case "DATE": .date
        default: .other
        }
    }

    /// Every whole occurrence of each original, as Rust's `match_indices`
    /// finds them (left to right, not overlapping), sorted by range.
    private static func candidates(_ text: String, _ identities: [ExportedIdentity]) -> [Candidate] {
        let haystack = Array(text.utf8)
        var found: [Candidate] = []
        for identity in identities {
            for original in identity.originals {
                let needle = Array(original.utf8)
                var start = 0
                while start + needle.count <= haystack.count {
                    if haystack[start..<(start + needle.count)].elementsEqual(needle) {
                        found.append(Candidate(
                            range: TextRange(start: UInt64(start), end: UInt64(start + needle.count)),
                            category: category(identity.category), score: 0.9))
                        start += needle.count
                    } else {
                        start += 1
                    }
                }
            }
        }
        return found.sorted { ($0.range.start, $0.range.end) < ($1.range.start, $1.range.end) }
    }

    /// Mentions as the goldens list them: range, state, alias, original.
    private static func mentions(_ snapshot: ReviewSnapshot) -> String {
        snapshot.mentions.map { mention in
            let state = switch mention.state {
            case .proposed: "Proposed"
            case .replaced: "Replaced"
            case .kept: "Kept"
            }
            return "\(mention.range.start)..\(mention.range.end)\t\(state)\t\(mention.alias ?? "-")\t\(mention.original)\n"
        }.joined()
    }

    func testSettlementDocxMatchesTheGoldens() throws {
        let docx = Self.repo.appendingPathComponent("docs/product-media/Settlement agreement.docx")
        let identities = try JSONDecoder().decode(
            [ExportedIdentity].self, from: Data(read("tests/fixtures/restore/claude-en/identities.json").utf8))
        let goldens = "tests/fixtures/mobile-parity/settlement-docx/"

        let session = DocumentSession()
        let opened = try session.importFile(path: docx.path, options: ImportOptions(ocr: nil, nativeTextOnly: false), progress: nil)
        XCTAssertEqual(opened.title, "Settlement agreement")
        XCTAssertEqual(opened.text, try read(goldens + "markdown.md"), "conversion differs from desktop's")

        let found = Self.candidates(opened.text, identities)
        XCTAssertGreaterThan(found.count, 20)
        let proposed = try session.addCandidates(candidates: found)
        XCTAssertTrue(proposed.mentions.allSatisfy { $0.state == .proposed })
        XCTAssertEqual(Self.mentions(proposed), try read(goldens + "candidates.tsv"))

        let replaced = try session.replaceAll()
        XCTAssertEqual(Self.mentions(replaced), try read(goldens + "replaced.tsv"))
        let copied = try session.copyForAi()
        XCTAssertEqual(copied, try read(goldens + "copy-for-ai.md"))
        XCTAssertEqual(try session.shareText(), copied)

        // An AI that answers with the document itself gets every original back.
        let body = try XCTUnwrap(copied.range(of: "\n\n---\n\n")).upperBound
        let reply = "Summary of the agreement:\n\n" + copied[body...]
        let restored = try session.restoreReply(reply: reply)
        XCTAssertEqual(restored.text, try read(goldens + "restored.md"))
        XCTAssertEqual(restored.unrecognised, 0)
    }

    /// The Restore extension's path: a saved document's case restores a
    /// reply exactly as the document's session does.
    func testLibraryRestoreMatchesTheSession() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("mdoc-parity-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let library = try MdocFFI.Library(root: root.path, caseKey: Data(repeating: 42, count: 32))
        let identities = try JSONDecoder().decode(
            [ExportedIdentity].self, from: Data(read("tests/fixtures/restore/claude-en/identities.json").utf8))
        let markdown = try read("docs/product-media/Settlement agreement.md")

        let session = DocumentSession()
        _ = try session.importText(text: markdown, title: "Settlement agreement")
        _ = try session.addCandidates(candidates: Self.candidates(markdown, identities))
        _ = try session.replaceAll()
        let saved = try session.save(library: library)
        let copied = try session.copyForAi()
        let reply = "Summary:\n\n" + copied[try XCTUnwrap(copied.range(of: "\n\n---\n\n")).upperBound...]
        let inSession = try session.restoreReply(reply: reply)
        let inExtension = try library.restore(caseId: saved.case.id, reply: reply)
        XCTAssertEqual(inExtension.text, inSession.text)
        // "Maya" alone joined her full name (ADR 0092), so it restores as that.
        XCTAssertEqual(
            inExtension.text,
            "Summary:\n\n" + markdown.replacingOccurrences(of: "Maya brought", with: "Maya Bennett brought"))
    }
}
