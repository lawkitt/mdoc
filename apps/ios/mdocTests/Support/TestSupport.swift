import Foundation
import MdocFFI
import XCTest
@testable import mdoc

enum Fixture {
    static let gigabyte: UInt64 = 1_073_741_824
    /// Below the detector floor: the core scans with its structured rules
    /// and no model, so the tests need none.
    static let belowFloor: UInt64 = 3 * gigabyte
    /// At the detector tier: a scan needs the name detector, which the
    /// tests never download, so it asks for setup.
    static let detectorTier: UInt64 = 8 * gigabyte

    /// Where every test's library goes. Most tests leave theirs; the first
    /// use in a run clears what earlier runs left.
    private static let temporaryFolder: URL = {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("mdoc-tests", isDirectory: true)
        try? FileManager.default.removeItem(at: folder)
        return folder
    }()

    static func temporaryStorage() throws -> MdocStorage {
        let root = temporaryFolder.appendingPathComponent(UUID().uuidString, isDirectory: true)
        let storage = MdocStorage(root: root)
        try storage.prepare()
        return storage
    }

    /// The repository, for its test fixtures: the Simulator reads the Mac's files.
    static let repository = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent()

    /// A copy of the repository's `path` in a folder of its own under the
    /// storage's intake folder, as the app's intake leaves a file: an import
    /// renames its file to the title.
    static func copy(_ path: String, into storage: MdocStorage) throws -> URL {
        let source = repository.appendingPathComponent(path)
        let folder = storage.url(.intake).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let copy = folder.appendingPathComponent(source.lastPathComponent)
        try FileManager.default.copyItem(at: source, to: copy)
        return copy
    }

    /// Preferences of a test's own, in a fresh store.
    static func preferences() -> Preferences {
        Preferences(store: UserDefaults(suiteName: "mdoc-tests-\(UUID().uuidString)")!)
    }

    /// The app's model over `seeded`, with preferences of its own.
    @MainActor
    static func app(_ seeded: Seeded) -> AppModel {
        AppModel(
            core: seeded.core, storage: seeded.storage,
            background: NoBackgroundContinuation(), downloader: FakeModelDownloader(seconds: 0),
            network: FakeNetwork(.wifi), preferences: preferences())
    }

    /// The real core over fresh temporary storage, with a fixed case key.
    static func core(memory: UInt64 = belowFloor) throws -> (Core, MdocStorage) {
        let storage = try temporaryStorage()
        return (try RealCore.make(storage: storage, keys: FixedVaultKeyStore(), physicalMemory: memory), storage)
    }

    /// The fictional library (`Letters`) on the real core: the quotation,
    /// replaced and shared, and the letter waiting for review in the Project
    /// "Ellison v Brightwater"; the tenancy scan, not yet recognized, in the
    /// Project "Harbour Street lease". The letter's and quotation's names
    /// are proposed as Try a sample proposes its own, so no model is needed.
    /// The Vault dates a document by its last save, to the second, and lists
    /// a tie by title: the letter, then the quotation (the tenancy, an
    /// import not yet read, dates from now).
    static func seeded(memory: UInt64 = belowFloor) throws -> Seeded {
        let (core, storage) = try core(memory: memory)
        let library = core.library
        let ellison = try createProject(Letters.ellisonProject, in: core)
        let harbour = try createProject(Letters.harbourProject, in: core)
        let pages = try Letters.pageImages(
            Letters.tenancyPages, named: "tenancy", in: storage.url(.intake).appendingPathComponent(UUID().uuidString))
        let tenancy = try library.importDocument(.scan(pages), title: Letters.tenancyTitle, into: harbour)
        let quotation = try library.importDocument(.text(Letters.quotation), title: Letters.quotationTitle, into: ellison)
        _ = try quotation.recognize(observer: TestObserver())
        _ = try quotation.propose(Letters.names)
        _ = try quotation.replaceAllPending()
        _ = try quotation.copyForAI()
        let letter = try library.importDocument(.text(Letters.letter), title: Letters.letterTitle, into: ellison)
        _ = try letter.recognize(observer: TestObserver())
        _ = try letter.propose(Letters.names)
        try library.saveChanges()
        return Seeded(
            core: core, storage: storage, ellison: ellison, harbour: harbour,
            letter: letter.id, quotation: quotation.id, tenancy: tenancy.id)
    }

    /// Every folder of the Vault made read-only, or writable again: a save
    /// then fails as one does while the phone is locked.
    static func setVault(_ storage: MdocStorage, writable: Bool) throws {
        let files = FileManager.default
        var folders = [storage.vault]
        if let items = files.enumerator(at: storage.vault, includingPropertiesForKeys: [.isDirectoryKey]) {
            for case let item as URL in items where (try? item.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true {
                folders.append(item)
            }
        }
        for folder in folders {
            try files.setAttributes([.posixPermissions: writable ? 0o700 : 0o500], ofItemAtPath: folder.path)
        }
    }

    /// The text a document has saved in the Vault, read through a session
    /// of its own.
    static func savedText(_ vaultID: String, in library: RealLibrary) throws -> String {
        try MdocFFI.DocumentSession().openFromVault(vault: library.vault, id: vaultID).text
    }

    /// A Project made in the Vault, as the Project page will make one
    /// (Vault & Projects item 11): no import makes one.
    static func createProject(_ name: String, in core: Core) throws -> ProjectID {
        let library = try XCTUnwrap(core.library as? RealLibrary)
        return ProjectID(rawValue: try library.vault.createProject(name: name).id)
    }
}

/// `Fixture.seeded()`: the real core and the ids of what it holds.
struct Seeded {
    let core: Core
    let storage: MdocStorage
    let ellison: ProjectID
    let harbour: ProjectID
    let letter: DocumentID
    let quotation: DocumentID
    let tenancy: DocumentID

    var library: any mdoc.Library { core.library }

    func session(_ id: DocumentID) throws -> any mdoc.DocumentSession {
        try core.library.session(for: id)
    }

    /// A document's view model over its real session; `slow` delays its
    /// long jobs (`SlowJobs`).
    @MainActor
    func model(
        _ id: DocumentID, jobs: LongJobRunner = LongJobRunner(background: NoBackgroundContinuation()),
        findsNames: Bool = true, slow seconds: Double? = nil
    ) throws -> DocumentViewModel {
        let session = try session(id)
        return DocumentViewModel(
            session: seconds.map { SlowJobs(session, seconds: $0) } ?? session, jobs: jobs, storage: storage,
            findsNames: findsNames)
    }
}

/// A connection the test controls.
@MainActor
final class FakeNetwork: NetworkConditions {
    private(set) var route: NetworkRoute
    var onChange: ((NetworkRoute) -> Void)?

    init(_ route: NetworkRoute) {
        self.route = route
    }

    func change(to route: NetworkRoute) {
        self.route = route
        onChange?(route)
    }
}

/// Holds a Wi-Fi-only download until it is cancelled, as the background
/// session does on cellular; a cellular one finishes at once.
final class HeldDownloader: ModelDownloader, @unchecked Sendable {
    private let lock = NSLock()
    private var calls: [Bool] = []

    /// `allowCellular` of each download started, in order.
    var cellular: [Bool] { lock.withLock { calls } }

    func download(
        _ request: ModelDownloadRequest,
        allowCellular: Bool,
        progress: @escaping @Sendable (Double) -> Void
    ) async throws -> URL {
        lock.withLock { calls.append(allowCellular) }
        if !allowCellular {
            try await Task.sleep(for: .seconds(600))
        }
        progress(1)
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("\(request.modelID)-\(UUID().uuidString).download")
        try Data().write(to: file)
        return file
    }
}

/// Records a job's progress and cancels on request.
final class TestObserver: JobObserver, @unchecked Sendable {
    private let lock = NSLock()
    private var cancelled = false
    private var handlers: [@Sendable () -> Void] = []
    private(set) var updates: [JobUpdate] = []
    /// Runs on the job's thread with each update, while the job runs.
    var onProgress: (@Sendable (JobUpdate) -> Void)?

    func cancel() {
        lock.lock()
        cancelled = true
        let handlers = self.handlers
        self.handlers = []
        lock.unlock()
        handlers.forEach { $0() }
    }

    func progress(_ update: JobUpdate) {
        lock.lock()
        updates.append(update)
        let hook = onProgress
        lock.unlock()
        hook?(update)
    }

    func onCancel(_ handler: @escaping @Sendable () -> Void) {
        lock.lock()
        if cancelled {
            lock.unlock()
            handler()
        } else {
            handlers.append(handler)
            lock.unlock()
        }
    }

    func isCancelled() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return cancelled
    }
}

/// The old library as the app left it before the Vault, which the Restore
/// extension still reads until Vault & Projects item 10: the quotation,
/// replaced and shared last, and the letter waiting for review in the case
/// "Ellison v Brightwater"; the tenancy in "Harbour Street lease". Written
/// through the core's old objects, as the app wrote it then.
struct LegacySeeded {
    let storage: MdocStorage
    let library: any mdoc.Library
    let ellison: ProjectID
    let harbour: ProjectID
    let letter: DocumentID
    let quotation: DocumentID
    let tenancy: DocumentID
    /// The alias the shared quotation gave Maya Ellison.
    let alias: String

    static func make() throws -> LegacySeeded {
        let storage = try Fixture.temporaryStorage()
        let key = try FixedVaultKeyStore().key()
        let old = try MdocFFI.Library(root: storage.root.path, caseKey: key)
        let ellison = try old.createCase(name: Letters.ellisonProject).id
        let harbour = try old.createCase(name: Letters.harbourProject).id
        let names = Letters.names.map { ProposedName(wording: $0.wording, category: $0.category) }
        var alias = ""
        func saved(_ text: String, _ title: String, in caseID: String, replace: Bool, share: Bool, hoursAgo: Double) throws -> DocumentID {
            let session = MdocFFI.DocumentSession()
            _ = try session.importTextAs(text: text, title: title, origin: .ordinary)
            _ = try session.joinCase(library: old, caseId: caseID)
            _ = try session.review(action: .proposeNames(names: names), library: nil)
            if replace {
                let review = try session.review(action: .replaceAll, library: nil)
                alias = review.mentions.first { $0.original == "Maya Ellison" }?.alias ?? ""
            }
            let path = try session.save(library: old).path
            if share { _ = try session.handOff(library: old) }
            // The old library dates a document by its file.
            try FileManager.default.setAttributes(
                [.modificationDate: Date().addingTimeInterval(-hoursAgo * 3600)], ofItemAtPath: path)
            return DocumentID(rawValue: path)
        }
        let quotation = try saved(Letters.quotation, Letters.quotationTitle, in: ellison, replace: true, share: true, hoursAgo: 2)
        let letter = try saved(Letters.letter, Letters.letterTitle, in: ellison, replace: false, share: false, hoursAgo: 1)
        let tenancy = try saved("Tenancy agreement", Letters.tenancyTitle, in: harbour, replace: false, share: false, hoursAgo: 3)
        return LegacySeeded(
            storage: storage, library: try RealCore.library(storage: storage, keys: FixedVaultKeyStore()),
            ellison: ProjectID(rawValue: ellison), harbour: ProjectID(rawValue: harbour),
            letter: letter, quotation: quotation, tenancy: tenancy, alias: alias)
    }
}
