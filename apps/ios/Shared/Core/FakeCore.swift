import Foundation

/// A stand-in for the Rust core that previews, tests and the app use until
/// `crates/mdoc-ffi` is wired in. It keeps its state in memory and fakes
/// recognition, detection and restore over fictional fixtures; none of this
/// logic ships once the real core is in.
enum FakeCore {
    struct Timing: Sendable {
        /// Seconds per OCR page, per scan and per model install.
        var page: Double
        var scan: Double
        var install: Double

        static let realistic = Timing(page: 0.8, scan: 2.0, install: 1.0)
        static let instant = Timing(page: 0, scan: 0, install: 0)
    }

    struct Options: Sendable {
        var timing: Timing = .realistic
        /// Models start installed (UI tests, previews of review).
        var modelsInstalled = false
        /// Where fixture originals are written; nil leaves them out.
        var originalsFolder: URL?
        /// Decides the device tier. Below the floor the app offers no Find
        /// names; a scan the tests run there finds the structured
        /// categories only, as the UI tests' seed does on the real core.
        var physicalMemory: UInt64 = ProcessInfo.processInfo.physicalMemory
    }

    static func make(_ options: Options = Options()) -> Core {
        let state = FakeState(options: options)
        return Core(library: FakeLibrary(state: state), setup: FakeSetup(state: state))
    }
}

// MARK: - State

private struct FakeIdentity {
    let id: IdentityID
    var category: Category
    var variants: [String]
    var alias: String
    var owner: IdentityID? = nil
    /// Renamed by the user: a category change keeps it, as in the core.
    var custom = false

    var primary: String { variants.max { $0.count < $1.count } ?? "" }
}

/// One undo step of the fake's history (ADR 0098).
private struct FakeStep {
    var kind: HistoryStepKind
    var subject: String
    var candidate: CandidateID?
    var identity: IdentityID?

    var label: String {
        let kind = switch kind {
        case .replace: "Replace"
        case .keep: "Keep"
        case .replaceAll: "Replace all"
        case .mark: "Mark"
        case .rename: "Rename"
        case .category: "Category"
        case .sameAs: "Same as"
        case .newAlias: "New alias"
        case .owner: "Belongs to"
        case .joinCase: "Join case"
        }
        return subject.isEmpty ? kind : "\(kind) · \(subject)"
    }
}

/// A state Undo or Redo reaches: the document and its case's identities.
private struct FakeSaved {
    var document: FakeDocument
    var identities: [FakeIdentity]
    /// The step that made this state.
    var step: FakeStep?
}

private struct FakeHistory {
    var undo: [FakeSaved] = []
    var redo: [FakeSaved] = []
    var step: FakeStep?
}

private struct FakeCase {
    let id: CaseID
    var name: String
    var colour: CaseColour
    var identities: [FakeIdentity]
}

private struct FakeMention {
    let id: CandidateID
    var identityID: IdentityID
    /// In the source text, which never changes.
    var range: SourceRange
    var state: MentionState
}

private struct FakeDocument {
    let id: DocumentID
    var title: String
    var caseID: CaseID?
    var kind: SourceKind
    var pages: [String]
    var pagesDone: Int
    var original: OriginalSource
    var scanned: Bool
    var mentions: [FakeMention]
    var reply: String?
    var updated: Date
    var warning: String? = nil

    var source: String { pages.prefix(pagesDone).joined(separator: "\n\n") }
    var recognized: Bool { pagesDone == pages.count }
}

/// One rendering of a document: the current Markdown and where each source
/// stretch landed in it.
private struct Rendering {
    var markdown = ""
    var candidates: [Candidate] = []
    /// Unchanged source stretches: (current start, source start, byte length).
    var plain: [(current: Int, source: Int, length: Int)] = []

    func sourceRange(of current: SourceRange) -> SourceRange? {
        for piece in plain where current.start >= piece.current && current.end <= piece.current + piece.length {
            let shift = piece.source - piece.current
            return SourceRange(start: current.start + shift, end: current.end + shift)
        }
        return nil
    }
}

private final class FakeState: @unchecked Sendable {
    private let lock = NSLock()
    let timing: FakeCore.Timing
    let tier: DeviceTier
    let physicalMemory: UInt64
    var cases: [FakeCase] = []
    var documents: [FakeDocument] = []
    var lastShared: DocumentID?
    var models: [String: ModelState] = [:]
    private var counter = 0

    init(options: FakeCore.Options) {
        timing = options.timing
        physicalMemory = options.physicalMemory
        tier = options.physicalMemory >= FakeSetup.detectorFloor ? .detector : .belowFloor
        let installed: ModelState = options.modelsInstalled ? .installed : .notInstalled
        models = [FakeSetup.detectorID: installed, FakeSetup.ocrID: installed]
        seed(originals: options.originalsFolder)
    }

    func locked<T>(_ body: () throws -> T) rethrows -> T {
        lock.lock()
        defer { lock.unlock() }
        return try body()
    }

    func nextID(_ prefix: String) -> String {
        counter += 1
        return "\(prefix)\(counter)"
    }

    // MARK: Seed

    private func seed(originals folder: URL?) {
        var letterOriginal = OriginalSource.none
        var tenancyOriginal = OriginalSource.none
        #if canImport(UIKit)
        if let folder, let written = try? FakeFixtures.writeOriginals(into: folder) {
            letterOriginal = .pdf(written.letter)
            tenancyOriginal = .images(written.tenancy)
        }
        #endif
        let ellison = CaseID(rawValue: "case-ellison")
        let harbour = CaseID(rawValue: "case-harbour")
        cases = [
            FakeCase(id: ellison, name: "Ellison v Brightwater", colour: .blue, identities: []),
            FakeCase(id: harbour, name: "Harbour Street lease", colour: .green, identities: []),
        ]
        let now = Date()
        let letter = FakeDocument(
            id: DocumentID(rawValue: "doc-letter"), title: FakeFixtures.letterTitle, caseID: ellison,
            kind: .pdf, pages: [FakeFixtures.letter], pagesDone: 1, original: letterOriginal,
            scanned: false, mentions: [], reply: nil, updated: now)
        let quotation = FakeDocument(
            id: DocumentID(rawValue: "doc-quotation"), title: FakeFixtures.quotationTitle, caseID: ellison,
            kind: .docx, pages: [FakeFixtures.quotation], pagesDone: 1, original: .none,
            scanned: false, mentions: [], reply: nil, updated: now.addingTimeInterval(-86_400))
        let tenancy = FakeDocument(
            id: DocumentID(rawValue: "doc-tenancy"), title: FakeFixtures.tenancyTitle, caseID: harbour,
            kind: .scan, pages: FakeFixtures.tenancyPages, pagesDone: 1, original: tenancyOriginal,
            scanned: false, mentions: [], reply: nil, updated: now.addingTimeInterval(-3 * 86_400))
        documents = [letter, quotation, tenancy]
        // The letter waits for review, the quotation was shared already, the
        // tenancy scan stopped after its first page.
        detect(in: letter.id)
        detect(in: quotation.id)
        if let index = index(of: quotation.id) {
            for mention in documents[index].mentions.indices {
                documents[index].mentions[mention].state = .replaced
            }
        }
        lastShared = quotation.id
    }

    // MARK: Lookups

    func index(of id: DocumentID) -> Int? {
        documents.firstIndex { $0.id == id }
    }

    func caseIndex(of id: CaseID?) -> Int? {
        guard let id else { return nil }
        return cases.firstIndex { $0.id == id }
    }

    func identity(_ id: IdentityID, caseID: CaseID?) -> FakeIdentity? {
        guard let index = caseIndex(of: caseID) else { return nil }
        return cases[index].identities.first { $0.id == id }
    }

    // MARK: Detection

    /// What the structured rules find without the detector.
    static let structured: Set<Category> = [.email, .url, .phone, .identity, .tax, .bank]

    /// Finds fixture entities in the recognized source, longest wording
    /// first, and adds them to the document's case. Below the device floor
    /// only the structured categories are found.
    func detect(in id: DocumentID) {
        guard let index = index(of: id) else { return }
        let source = documents[index].source
        let caseID = documents[index].caseID
        var taken: [SourceRange] = []
        var mentions: [FakeMention] = []
        let entities = tier == .detector
            ? FakeFixtures.entities
            : FakeFixtures.entities.filter { Self.structured.contains($0.category) }
        let wordings = entities.flatMap { entity in entity.variants.map { (entity, $0) } }
            .sorted { $0.1.utf8.count > $1.1.utf8.count }
        for (entity, wording) in wordings {
            for range in Self.occurrences(of: wording, in: source) where !taken.contains(where: { $0.overlaps(range) }) {
                let identity = caseIdentity(for: entity, caseID: caseID)
                taken.append(range)
                mentions.append(FakeMention(
                    id: CandidateID(rawValue: nextID("m")), identityID: identity.id, range: range,
                    state: .proposed))
            }
        }
        documents[index].mentions = mentions.sorted { $0.range.start < $1.range.start }
        documents[index].scanned = true
        documents[index].updated = Date()
    }

    /// The case's identity for a fixture entity, created with the fixture's
    /// alias the first time.
    private func caseIdentity(for entity: FakeFixtures.Entity, caseID: CaseID?) -> FakeIdentity {
        if let caseIndex = caseIndex(of: caseID),
           let existing = cases[caseIndex].identities.first(where: { $0.variants.contains(entity.variants[0]) }) {
            return existing
        }
        let identity = FakeIdentity(
            id: IdentityID(rawValue: nextID("i")), category: entity.category,
            variants: entity.variants, alias: entity.alias)
        if let caseIndex = caseIndex(of: caseID) {
            cases[caseIndex].identities.append(identity)
        } else {
            orphanIdentities.append(identity)
        }
        return identity
    }

    /// Identities of documents without a case (none in the fixtures).
    var orphanIdentities: [FakeIdentity] = []

    // MARK: Review bar (ADR 0098)

    var histories: [DocumentID: FakeHistory] = [:]
    /// Dismissed merge hints, as (from, into) identity ids.
    var dismissedHints: Set<[String]> = []

    func identities(of caseID: CaseID?) -> [FakeIdentity] {
        caseIndex(of: caseID).map { cases[$0].identities } ?? orphanIdentities
    }

    func setIdentities(_ identities: [FakeIdentity], of caseID: CaseID?) {
        if let index = caseIndex(of: caseID) {
            cases[index].identities = identities
        } else {
            orphanIdentities = identities
        }
    }

    func updateIdentity(_ id: IdentityID, of caseID: CaseID?, _ body: (inout FakeIdentity) throws -> Void) throws {
        var identities = identities(of: caseID)
        guard let index = identities.firstIndex(where: { $0.id == id }) else { throw CoreError.notFound }
        try body(&identities[index])
        setIdentities(identities, of: caseID)
    }

    /// The next free `TOKEN_n` in the case.
    func nextAlias(_ category: Category, caseID: CaseID?) -> String {
        let taken = Set(identities(of: caseID).map(\.alias))
        var number = 1
        while taken.contains("\(category.token)_\(number)") { number += 1 }
        return "\(category.token)_\(number)"
    }

    func addIdentity(_ category: Category, alias: String? = nil, variants: [String], caseID: CaseID?) -> FakeIdentity {
        let identity = FakeIdentity(
            id: IdentityID(rawValue: nextID("i")), category: category, variants: variants,
            alias: alias ?? nextAlias(category, caseID: caseID))
        setIdentities(identities(of: caseID) + [identity], of: caseID)
        return identity
    }

    /// Run `change` on document `id` as one undo step; it clears Redo.
    func record(_ id: DocumentID, _ step: FakeStep, _ change: (Int) throws -> Void) throws {
        guard let index = index(of: id) else { throw CoreError.notFound }
        let caseID = documents[index].caseID
        let saved = FakeSaved(document: documents[index], identities: identities(of: caseID), step: histories[id]?.step)
        try change(index)
        documents[index].updated = Date()
        var history = histories[id] ?? FakeHistory()
        history.undo.append(saved)
        if history.undo.count > 256 { history.undo.removeFirst() }
        history.redo = []
        history.step = step
        histories[id] = history
    }

    /// Undo (`back`) or Redo; nothing to travel to changes nothing.
    func travel(_ id: DocumentID, back: Bool) throws {
        guard let index = index(of: id) else { throw CoreError.notFound }
        var history = histories[id] ?? FakeHistory()
        guard let target = back ? history.undo.popLast() : history.redo.popLast() else { return }
        let caseID = documents[index].caseID
        let current = FakeSaved(document: documents[index], identities: identities(of: caseID), step: history.step)
        if back { history.redo.append(current) } else { history.undo.append(current) }
        // The reply is no review step: it stays as it is now.
        let reply = documents[index].reply
        documents[index] = target.document
        documents[index].reply = reply
        setIdentities(target.identities, of: caseID)
        history.step = target.step
        histories[id] = history
    }

    /// Two people or organizations whose wordings end alike look like one
    /// subject: the shorter hints at the fuller (ADR 0080, roughly).
    func mergeHints(_ identities: [Identity]) -> [MergeHint] {
        let last = { (wording: String) in wording.split(separator: " ").last.map { $0.lowercased() } }
        var hints: [MergeHint] = []
        for from in identities where [.person, .organization].contains(from.category) {
            let into = identities.first { other in
                other.id != from.id && other.category == from.category
                    && other.primaryWording.count > from.primaryWording.count
                    && last(other.primaryWording) == last(from.primaryWording)
            }
            if let into, !dismissedHints.contains([from.id.rawValue, into.id.rawValue]) {
                hints.append(MergeHint(from: from.id, into: into.id, wording: into.primaryWording))
            }
        }
        return hints
    }

    func historyStep(_ step: FakeStep?, rendering: Rendering) -> HistoryStep? {
        guard let step else { return nil }
        let candidate = step.candidate.flatMap { id in rendering.candidates.first { $0.id == id } }
        return HistoryStep(
            kind: step.kind, label: step.label, subject: step.subject, range: candidate?.range,
            candidateID: candidate?.id, identityID: candidate?.identityID ?? step.identity)
    }

    static func occurrences(of needle: String, in haystack: String) -> [SourceRange] {
        var result: [SourceRange] = []
        var searchStart = haystack.startIndex
        while let found = haystack.range(of: needle, range: searchStart..<haystack.endIndex) {
            let start = haystack.utf8.distance(from: haystack.utf8.startIndex, to: found.lowerBound)
            result.append(SourceRange(start: start, end: start + needle.utf8.count))
            searchStart = found.upperBound
        }
        return result
    }

    // MARK: Rendering

    func render(_ document: FakeDocument) -> Rendering {
        let source = Array(document.source.utf8)
        var rendering = Rendering()
        var bytes: [UInt8] = []
        var cursor = 0
        for mention in document.mentions.sorted(by: { $0.range.start < $1.range.start }) {
            guard let identity = identity(mention.identityID, caseID: document.caseID)
                ?? orphanIdentities.first(where: { $0.id == mention.identityID }) else { continue }
            rendering.plain.append((bytes.count, cursor, mention.range.start - cursor))
            bytes += source[cursor..<mention.range.start]
            let original = String(decoding: source[mention.range.start..<mention.range.end], as: UTF8.self)
            let alias = identity.alias
            let shown = mention.state == .replaced ? alias : original
            let start = bytes.count
            bytes += Array(shown.utf8)
            rendering.candidates.append(Candidate(
                id: mention.id, identityID: identity.id,
                range: SourceRange(start: start, end: bytes.count), original: original,
                alias: alias, category: identity.category, state: mention.state))
            cursor = mention.range.end
        }
        rendering.plain.append((bytes.count, cursor, source.count - cursor))
        bytes += source[cursor...]
        rendering.markdown = String(decoding: bytes, as: UTF8.self)
        return rendering
    }

    func snapshot(_ id: DocumentID) throws -> DocumentSnapshot {
        guard let index = index(of: id) else { throw CoreError.notFound }
        let document = documents[index]
        let rendering = render(document)
        let caseIdentities = caseIndex(of: document.caseID).map { cases[$0].identities } ?? orphanIdentities
        let identities: [Identity] = caseIdentities.compactMap { identity in
            let mentions = rendering.candidates.filter { $0.identityID == identity.id }
            guard !mentions.isEmpty else { return nil }
            let states = Set(mentions.map(\.state))
            let state: MentionState = states.contains(.proposed) ? .proposed : (states == [.kept] ? .kept : .replaced)
            return Identity(
                id: identity.id, alias: identity.alias, category: identity.category,
                primaryWording: identity.primary, variants: identity.variants,
                mentionCount: mentions.count, state: state, owner: identity.owner)
        }
        let history = histories[id] ?? FakeHistory()
        let phase: DocumentPhase
        if !document.recognized {
            phase = .recognizing
        } else if !document.scanned {
            phase = .unscanned
        } else if rendering.candidates.contains(where: { $0.state == .proposed }) {
            phase = .reviewing
        } else {
            phase = .ready
        }
        let reply = try document.reply.map { reply in
            ReplySnapshot(aliasText: reply, restored: try restore(reply, caseID: document.caseID))
        }
        return DocumentSnapshot(
            id: document.id, title: document.title, caseID: document.caseID,
            caseName: caseIndex(of: document.caseID).map { cases[$0].name },
            kind: document.kind, phase: phase, markdown: rendering.markdown,
            styling: MarkdownStyling(markdown: rendering.markdown), candidates: rendering.candidates,
            identities: identities, original: document.original,
            pagesDone: document.pagesDone, pageCount: document.pages.count, reply: reply,
            warning: document.recognized ? document.warning : nil,
            mergeHints: mergeHints(identities),
            canUndo: !history.undo.isEmpty, canRedo: !history.redo.isEmpty,
            undoStep: history.undo.isEmpty ? nil : historyStep(history.step, rendering: rendering),
            redoStep: historyStep(history.redo.last?.step, rendering: rendering))
    }

    func summary(_ document: FakeDocument) -> DocumentSummary {
        let snapshot = try? snapshot(document.id)
        return DocumentSummary(
            id: document.id, title: document.title, caseID: document.caseID, kind: document.kind,
            phase: snapshot?.phase ?? .recognizing, pendingCount: snapshot?.pendingCount ?? 0,
            updated: document.updated, candidateCount: snapshot?.candidates.count ?? 0)
    }

    // MARK: Restore

    private static let aliasPattern = try! NSRegularExpression(
        pattern: #"\b(?<prefix>PERSON|ORG|EMAIL|URL|PHONE|ADDRESS|IDENTITY|TAX|BANK|CASE|DATE|REDACTED)(?<separator>[_\- ])(?<number>\d+)\b"#,
        options: [.caseInsensitive])

    /// Lenient on case and `_`/`-`/space separators; a loose form counts
    /// only when it names a known alias, and nothing is guessed.
    func restore(_ reply: String, caseID: CaseID?) throws -> RestoreResult {
        guard let caseIndex = caseIndex(of: caseID) else { throw CoreError.noCase }
        var known: [String: String] = [:]
        for identity in cases[caseIndex].identities {
            known[identity.alias.uppercased()] = identity.primary
        }
        let text = reply as NSString
        var segments: [RestoredSegment] = []
        var cursor = 0
        for match in Self.aliasPattern.matches(in: reply, range: NSRange(location: 0, length: text.length)) {
            let token = text.substring(with: match.range)
            let prefix = text.substring(with: match.range(withName: "prefix"))
            let separator = text.substring(with: match.range(withName: "separator"))
            let number = text.substring(with: match.range(withName: "number"))
            let segment: RestoredSegment
            if let value = known["\(prefix.uppercased())_\(number)"] {
                segment = .restored(value: value, alias: token)
            } else if separator == "_" && prefix == prefix.uppercased() {
                segment = .unresolved(token)
            } else {
                continue
            }
            if match.range.location > cursor {
                segments.append(.text(text.substring(with: NSRange(location: cursor, length: match.range.location - cursor))))
            }
            segments.append(segment)
            cursor = match.range.location + match.range.length
        }
        if cursor < text.length {
            segments.append(.text(text.substring(from: cursor)))
        }
        return RestoreResult(segments: segments)
    }
}

private extension SourceRange {
    func overlaps(_ other: SourceRange) -> Bool {
        start < other.end && other.start < end
    }
}

/// Sleeps in short slices so cancellation lands quickly.
private func work(seconds: Double, observer: any JobObserver) throws {
    guard seconds > 0 else {
        if observer.isCancelled() { throw CoreError.cancelled }
        return
    }
    let slices = max(1, Int(seconds / 0.05))
    for _ in 0..<slices {
        if observer.isCancelled() { throw CoreError.cancelled }
        Thread.sleep(forTimeInterval: seconds / Double(slices))
    }
}

// MARK: - Library

private final class FakeLibrary: Library {
    let state: FakeState

    init(state: FakeState) {
        self.state = state
    }

    func snapshot() -> LibrarySnapshot {
        state.locked {
            LibrarySnapshot(
                cases: state.cases.map { CaseSummary(id: $0.id, name: $0.name, colour: $0.colour) },
                documents: state.documents.map(state.summary).sorted { $0.updated > $1.updated },
                currentCase: state.lastShared.flatMap { id in state.documents.first { $0.id == id }?.caseID })
        }
    }

    func session(for document: DocumentID) throws -> any DocumentSession {
        try state.locked {
            guard state.index(of: document) != nil else { throw CoreError.notFound }
        }
        return FakeSession(state: state, id: document)
    }

    func importDocument(_ source: ImportSource, title: String, into caseChoice: CaseChoice) throws -> any DocumentSession {
        let id = try state.locked { () throws -> DocumentID in
            let caseID: CaseID
            switch caseChoice {
            case .existing(let existing):
                guard state.caseIndex(of: existing) != nil else { throw CoreError.notFound }
                caseID = existing
            case .newCase(let name):
                caseID = CaseID(rawValue: state.nextID("case-"))
                let colour = CaseColour.allCases[state.cases.count % CaseColour.allCases.count]
                state.cases.append(FakeCase(id: caseID, name: name, colour: colour, identities: []))
            }
            let kind: SourceKind
            let pages: [String]
            let original: OriginalSource
            switch source {
            case .text(let text):
                kind = .text
                pages = [text]
                original = .none
            case .file(let url, let fileKind):
                kind = fileKind
                pages = [FakeFixtures.importedText(title: title)]
                original = fileKind == .pdf ? .pdf(url) : (fileKind == .image ? .images([url]) : .none)
            case .scan(let urls), .photos(let urls):
                if case .scan = source { kind = .scan } else { kind = .image }
                pages = urls.indices.map { page in
                    page == 0 ? FakeFixtures.importedText(title: title) : "Page \(page + 1)."
                }
                original = .images(urls)
            }
            let id = DocumentID(rawValue: state.nextID("doc-"))
            // Pasted text needs no recognition.
            let pagesDone = kind == .text ? pages.count : 0
            state.documents.append(FakeDocument(
                id: id, title: title, caseID: caseID, kind: kind, pages: pages, pagesDone: pagesDone,
                original: original, scanned: false, mentions: [], reply: nil, updated: Date(),
                warning: kind == .pdf ? FakeFixtures.importWarning : nil))
            return id
        }
        return FakeSession(state: state, id: id)
    }

    func deleteDocument(_ document: DocumentID) throws {
        try state.locked {
            guard let index = state.index(of: document) else { throw CoreError.notFound }
            state.documents.remove(at: index)
            if state.lastShared == document {
                state.lastShared = nil
            }
        }
    }

    func restore(reply: String, in caseID: CaseID) throws -> RestoreResult {
        try state.locked { try state.restore(reply, caseID: caseID) }
    }

    func saveReply(_ reply: String, to document: DocumentID) throws {
        try state.locked {
            guard let index = state.index(of: document) else { throw CoreError.notFound }
            state.documents[index].reply = reply
            state.documents[index].updated = Date()
        }
    }
}

// MARK: - Session

private final class FakeSession: DocumentSession {
    let state: FakeState
    let id: DocumentID

    init(state: FakeState, id: DocumentID) {
        self.state = state
        self.id = id
    }

    func snapshot() -> DocumentSnapshot {
        state.locked {
            (try? state.snapshot(id)) ?? DocumentSnapshot(
                id: id, title: "", caseID: nil, caseName: nil, kind: .text, phase: .recognizing,
                markdown: "", styling: .empty, candidates: [], identities: [], original: .none,
                pagesDone: 0, pageCount: 0, reply: nil)
        }
    }

    private func mutate(_ body: (inout FakeDocument) throws -> Void) throws -> DocumentSnapshot {
        try state.locked {
            guard let index = state.index(of: id) else { throw CoreError.notFound }
            try body(&state.documents[index])
            state.documents[index].updated = Date()
            return try state.snapshot(id)
        }
    }

    func recognize(observer: any JobObserver) throws -> DocumentSnapshot {
        // Page images need OCR, as with the real core.
        let needsOCR = try state.locked { () throws -> Bool in
            guard let index = state.index(of: id) else { throw CoreError.notFound }
            let document = state.documents[index]
            return !document.recognized && (document.kind == .scan || document.kind == .image)
                && state.models[FakeSetup.ocrID] != .installed
        }
        if needsOCR { throw CoreError.modelMissing }
        while true {
            let (done, total) = try state.locked { () throws -> (Int, Int) in
                guard let index = state.index(of: id) else { throw CoreError.notFound }
                return (state.documents[index].pagesDone, state.documents[index].pages.count)
            }
            observer.progress(JobUpdate(
                stage: .recognizing, fraction: total == 0 ? 1 : Double(done) / Double(total), page: done, pageCount: total))
            if done >= total { break }
            try work(seconds: state.timing.page, observer: observer)
            // A finished page is kept even if the next one is cancelled.
            _ = try mutate { $0.pagesDone = min($0.pagesDone + 1, $0.pages.count) }
        }
        return snapshot()
    }

    func scan(observer: any JobObserver) throws -> DocumentSnapshot {
        // Below the floor only tests scan: the structured rules, no model.
        let ready = state.locked { state.tier == .belowFloor || state.models[FakeSetup.detectorID] == .installed }
        guard ready else { throw CoreError.modelMissing }
        let steps = 10
        for step in 0..<steps {
            observer.progress(JobUpdate(stage: .scanning, fraction: Double(step) / Double(steps)))
            try work(seconds: state.timing.scan / Double(steps), observer: observer)
        }
        observer.progress(JobUpdate(stage: .scanning, fraction: 1))
        return try state.locked {
            state.detect(in: id)
            return try state.snapshot(id)
        }
    }

    func decide(_ candidate: CandidateID, _ decision: Decision, scope: Scope) throws -> DocumentSnapshot {
        try state.locked {
            let (_, mention) = try located(candidate)
            let step = FakeStep(
                kind: decision == .replace ? .replace : .keep, subject: mention.original, candidate: candidate,
                identity: mention.identityID)
            try state.record(id, step) { index in
                let document = state.documents[index]
                for target in document.mentions.indices
                where scope == .justThisOne
                    ? document.mentions[target].id == candidate
                    : document.mentions[target].identityID == mention.identityID {
                    state.documents[index].mentions[target].state = decision == .replace ? .replaced : .kept
                }
            }
            return try state.snapshot(id)
        }
    }

    func replaceAllPending() throws -> DocumentSnapshot {
        try state.locked {
            let pending = try state.snapshot(id).pendingCount
            guard pending > 0 else { return try state.snapshot(id) }
            try state.record(id, FakeStep(kind: .replaceAll, subject: String(pending))) { index in
                for mention in state.documents[index].mentions.indices
                where state.documents[index].mentions[mention].state == .proposed {
                    state.documents[index].mentions[mention].state = .replaced
                }
            }
            return try state.snapshot(id)
        }
    }

    func addReplacement(_ range: SourceRange, category: Category) throws -> DocumentSnapshot {
        try mark(range, category: category, state: .replaced)
    }

    func markSelection(_ range: SourceRange) throws -> DocumentSnapshot {
        try mark(range, category: nil, state: .proposed)
    }

    func propose(_ names: [SampleDocument.Name]) throws -> DocumentSnapshot {
        for name in names.sorted(by: { $0.wording.count > $1.wording.count }) {
            let text = snapshot().markdown
            guard let range = text.range(of: name.wording) else { continue }
            let start = text.utf8.distance(from: text.startIndex, to: range.lowerBound)
            _ = try? mark(SourceRange(start: start, end: start + name.wording.utf8.count), category: name.category, state: .proposed)
        }
        return snapshot()
    }

    /// A manual identity for the selection, in every exact repeat; without
    /// a category, a rough guess as the core's.
    private func mark(_ range: SourceRange, category: Category?, state mentionState: MentionState) throws -> DocumentSnapshot {
        try state.locked {
            guard let index = state.index(of: id) else { throw CoreError.notFound }
            let document = state.documents[index]
            let rendering = state.render(document)
            guard range.length > 0, let sourceRange = rendering.sourceRange(of: range) else {
                throw CoreError.emptySelection
            }
            let source = document.source
            let utf8 = Array(source.utf8)
            let wording = String(decoding: utf8[sourceRange.start..<sourceRange.end], as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard !wording.isEmpty else { throw CoreError.emptySelection }
            let category = category ?? (wording.contains("@") ? .email : .person)
            let first = CandidateID(rawValue: state.nextID("m"))
            try state.record(id, FakeStep(kind: .mark, subject: wording, candidate: first)) { index in
                let identity = state.addIdentity(category, variants: [wording], caseID: document.caseID)
                var mentionID: CandidateID? = first
                for repeatRange in FakeState.occurrences(of: wording, in: source)
                where !document.mentions.contains(where: { $0.range.overlaps(repeatRange) }) {
                    state.documents[index].mentions.append(FakeMention(
                        id: mentionID ?? CandidateID(rawValue: state.nextID("m")), identityID: identity.id,
                        range: repeatRange, state: mentionState))
                    mentionID = nil
                }
                state.documents[index].mentions.sort { $0.range.start < $1.range.start }
                state.documents[index].scanned = true
            }
            return try state.snapshot(id)
        }
    }

    func undo() throws -> DocumentSnapshot {
        try state.locked {
            try state.travel(id, back: true)
            return try state.snapshot(id)
        }
    }

    func redo() throws -> DocumentSnapshot {
        try state.locked {
            try state.travel(id, back: false)
            return try state.snapshot(id)
        }
    }

    func rename(_ candidate: CandidateID, scope: Scope, to alias: String) throws -> DocumentSnapshot {
        try state.locked {
            let (document, mention) = try located(candidate)
            let value = alias.trimmingCharacters(in: .whitespaces)
            let identities = state.identities(of: document.caseID)
            guard let current = identities.first(where: { $0.id == mention.identityID }) else { throw CoreError.notFound }
            if current.alias == value { return try state.snapshot(id) }
            if let other = identities.first(where: { $0.alias == value && $0.id != current.id }) {
                throw CoreError.aliasTaken(alias: value, by: other.id)
            }
            let allowed = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-")
            guard !value.isEmpty, value.count <= 128, value.unicodeScalars.allSatisfy(allowed.contains) else {
                throw CoreError.refused("Use an ASCII alias with letters, digits, underscores or hyphens (up to 128 characters).")
            }
            try state.record(id, FakeStep(kind: .rename, subject: value, candidate: candidate)) { index in
                if scope == .allOccurrences {
                    try state.updateIdentity(current.id, of: document.caseID) {
                        $0.alias = value
                        $0.custom = true
                    }
                } else {
                    let split = state.addIdentity(
                        current.category, alias: value, variants: [mention.original], caseID: document.caseID)
                    try state.updateIdentity(split.id, of: document.caseID) { $0.custom = true }
                    self.split(index, candidate, into: split)
                }
            }
            return try state.snapshot(id)
        }
    }

    func categoryChoices(for candidate: CandidateID, scope: Scope) throws -> [CategoryChoice] {
        try state.locked {
            let (document, mention) = try located(candidate)
            let custom = state.identities(of: document.caseID).first { $0.id == mention.identityID }?.custom ?? false
            // As `category_choice_alias`: the whole identity keeps its alias
            // in its category, or when the user chose it; a split gets one.
            return Category.allCases.map { category in
                let keeps = scope == .allOccurrences && (category == mention.category || custom)
                return CategoryChoice(
                    category: category, alias: keeps ? nil : state.nextAlias(category, caseID: document.caseID),
                    isCurrent: category == mention.category)
            }
        }
    }

    func setCategory(_ candidate: CandidateID, scope: Scope, to category: Category) throws -> DocumentSnapshot {
        try state.locked {
            let (document, mention) = try located(candidate)
            if scope == .allOccurrences && category == mention.category { return try state.snapshot(id) }
            try state.record(id, FakeStep(kind: .category, subject: category.label, candidate: candidate)) { index in
                if scope == .allOccurrences {
                    let alias = state.nextAlias(category, caseID: document.caseID)
                    try state.updateIdentity(mention.identityID, of: document.caseID) {
                        $0.category = category
                        if !$0.custom { $0.alias = alias }
                        $0.owner = nil
                    }
                } else {
                    split(index, candidate, into: state.addIdentity(
                        category, variants: [mention.original], caseID: document.caseID))
                }
            }
            return try state.snapshot(id)
        }
    }

    func sameAs(_ candidate: CandidateID, scope: Scope, target: IdentityID) throws -> DocumentSnapshot {
        try state.locked {
            let (document, mention) = try located(candidate)
            guard let into = state.identities(of: document.caseID).first(where: { $0.id == target }) else {
                throw CoreError.notFound
            }
            // As the core: This one on a kept mention changes nothing.
            if scope == .justThisOne && mention.state == .kept { return try state.snapshot(id) }
            try state.record(id, FakeStep(kind: .sameAs, subject: into.primary, candidate: candidate, identity: target)) { index in
                if scope == .allOccurrences {
                    try merge(index, mention.identityID, into: target)
                } else {
                    split(index, candidate, into: into)
                }
            }
            return try state.snapshot(id)
        }
    }

    func link(_ identity: IdentityID, into target: IdentityID) throws -> DocumentSnapshot {
        try state.locked {
            guard let index = state.index(of: id),
                  let into = state.identities(of: state.documents[index].caseID).first(where: { $0.id == target }) else {
                throw CoreError.notFound
            }
            try state.record(id, FakeStep(kind: .sameAs, subject: into.primary, identity: target)) { index in
                try merge(index, identity, into: target)
            }
            return try state.snapshot(id)
        }
    }

    func newAlias(_ candidate: CandidateID) throws -> DocumentSnapshot {
        try state.locked {
            let (document, mention) = try located(candidate)
            if mention.state == .kept { return try state.snapshot(id) }
            let alias = state.nextAlias(mention.category, caseID: document.caseID)
            try state.record(id, FakeStep(kind: .newAlias, subject: alias, candidate: candidate)) { index in
                split(index, candidate, into: state.addIdentity(
                    mention.category, alias: alias, variants: [mention.original], caseID: document.caseID))
            }
            return try state.snapshot(id)
        }
    }

    func setOwner(_ identity: IdentityID, to owner: IdentityID?) throws -> DocumentSnapshot {
        try state.locked {
            guard let index = state.index(of: id) else { throw CoreError.notFound }
            let caseID = state.documents[index].caseID
            let identities = state.identities(of: caseID)
            guard let owned = identities.first(where: { $0.id == identity }) else { throw CoreError.notFound }
            let people: Set<Category> = [.person, .organization]
            if people.contains(owned.category) {
                throw CoreError.refused("Assign ownership to a contact, address or identifier.")
            }
            let ownerIdentity = owner.flatMap { owner in identities.first { $0.id == owner } }
            if owner != nil && !(ownerIdentity.map { people.contains($0.category) } ?? false) {
                throw CoreError.refused("Choose a person or organization as owner.")
            }
            if owned.owner == owner { return try state.snapshot(id) }
            let subject = ownerIdentity?.primary ?? "none"
            try state.record(id, FakeStep(kind: .owner, subject: subject, identity: identity)) { _ in
                try state.updateIdentity(identity, of: caseID) { $0.owner = owner }
            }
            return try state.snapshot(id)
        }
    }

    func dismissHint(_ hint: MergeHint) throws -> DocumentSnapshot {
        try state.locked {
            state.dismissedHints.insert([hint.from.rawValue, hint.into.rawValue])
            return try state.snapshot(id)
        }
    }

    /// The document and its candidate, as the snapshot shows it.
    private func located(_ candidate: CandidateID) throws -> (FakeDocument, Candidate) {
        guard let index = state.index(of: id),
              let mention = try state.snapshot(id).candidate(candidate) else { throw CoreError.notFound }
        return (state.documents[index], mention)
    }

    /// Moves one mention to `identity`.
    private func split(_ index: Int, _ candidate: CandidateID, into identity: FakeIdentity) {
        guard let mention = state.documents[index].mentions.firstIndex(where: { $0.id == candidate }) else { return }
        state.documents[index].mentions[mention].identityID = identity.id
    }

    /// Moves every mention of `from` to `target`, which takes its wordings.
    private func merge(_ index: Int, _ from: IdentityID, into target: IdentityID) throws {
        guard from != target else { return }
        let caseID = state.documents[index].caseID
        let variants = state.identities(of: caseID).first { $0.id == from }?.variants ?? []
        try state.updateIdentity(target, of: caseID) { identity in
            identity.variants += variants.filter { !identity.variants.contains($0) }
        }
        for mention in state.documents[index].mentions.indices
        where state.documents[index].mentions[mention].identityID == from {
            state.documents[index].mentions[mention].identityID = target
        }
    }

    func copyForAI() throws -> HandOff {
        try state.locked {
            let snapshot = try state.snapshot(id)
            guard snapshot.pendingCount == 0 else { throw CoreError.pendingReview(snapshot.pendingCount) }
            var examples: [String] = []
            for candidate in snapshot.candidates where candidate.state == .replaced && !examples.contains(candidate.alias) {
                examples.append(candidate.alias)
            }
            let named = switch examples.count {
            case 0: "PERSON_1"
            case 1: examples[0]
            default: "\(examples[0]) or \(examples[1])"
            }
            let note = "Note: names and other identifying details in this document were replaced "
                + "with placeholders such as \(named). Keep every placeholder exactly as "
                + "written, with the same capitals, underscore and number. Do not invent new "
                + "placeholders or guess the real values; they are restored locally after "
                + "your reply.\n\n---\n\n"
            var fileName = snapshot.title
            for identity in snapshot.identities where identity.state != .kept {
                for variant in identity.variants {
                    fileName = fileName.replacingOccurrences(of: variant, with: identity.alias)
                }
            }
            state.lastShared = id
            return HandOff(text: note + snapshot.markdown, fileName: fileName + ".md")
        }
    }

    func restoreReply(_ reply: String) throws -> DocumentSnapshot {
        try state.locked {
            guard let index = state.index(of: id) else { throw CoreError.notFound }
            // Restore reads every document of the case, so it runs before
            // this document is written.
            _ = try state.restore(reply, caseID: state.documents[index].caseID)
            state.documents[index].reply = reply
            state.documents[index].updated = Date()
            return try state.snapshot(id)
        }
    }

    func clearReply() throws -> DocumentSnapshot {
        try mutate { $0.reply = nil }
    }
}

// MARK: - Setup

private final class FakeSetup: Setup {
    static let detectorID = "gliner2-multi-fp16"
    static let ocrID = "pp-ocr-v5"
    /// The real core's floor (`DETECTOR_FLOOR_BYTES`, 7 GiB).
    static let detectorFloor: UInt64 = 7 * 1_073_741_824

    let state: FakeState

    init(state: FakeState) {
        self.state = state
    }

    func snapshot() -> SetupSnapshot {
        state.locked {
            var offers = [
                ModelOffer(
                    id: Self.ocrID, name: String(localized: "Text recognition (PP-OCR)"), purpose: .ocr,
                    byteSize: 31_000_000, state: state.models[Self.ocrID] ?? .notInstalled),
            ]
            if state.tier == .detector {
                offers.insert(ModelOffer(
                    id: Self.detectorID, name: String(localized: "Name detector (GLiNER2)"), purpose: .detector,
                    byteSize: 1_190_000_000, state: state.models[Self.detectorID] ?? .notInstalled), at: 0)
            }
            return SetupSnapshot(physicalMemory: state.physicalMemory, tier: state.tier, offers: offers)
        }
    }

    func downloadRequests(for model: String) throws -> [ModelDownloadRequest] {
        guard model == Self.detectorID || model == Self.ocrID else { throw CoreError.notFound }
        let size: Int64 = model == Self.detectorID ? 1_190_000_000 : 31_000_000
        // A reserved example host: the fake downloader never fetches it.
        return [ModelDownloadRequest(
            modelID: model, file: "model.onnx", url: URL(string: "https://models.example.invalid/\(model)")!,
            byteSize: size)]
    }

    func install(_ request: ModelDownloadRequest, from file: URL) throws -> SetupSnapshot {
        if state.timing.install > 0 {
            Thread.sleep(forTimeInterval: state.timing.install)
        }
        state.locked { state.models[request.modelID] = .installed }
        return snapshot()
    }
}
