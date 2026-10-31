import Foundation
import Observation

/// The first-use setup card and Settings' model rows: the models that fit
/// this device by physical RAM, their size, and the download (ADR 0026,
/// ADR 0085). Wi-Fi by default; on any cellular interface, a download asks
/// first, and one held for Wi-Fi says so (ADR 0088).
@MainActor
@Observable
final class SetupViewModel {
    private let setup: any Setup
    private let downloader: any ModelDownloader
    private let network: any NetworkConditions
    private let jobs: LongJobRunner
    private let defaults: UserDefaults

    private(set) var snapshot: SetupSnapshot
    /// Download progress the platform owns until the core installs the file.
    private(set) var downloadProgress: [String: Double] = [:]
    private(set) var errorMessage: String?
    /// The cellular question is showing.
    var asksCellular = false
    private(set) var isWorking = false
    /// The connection now.
    private(set) var route: NetworkRoute
    /// The running download may use cellular: the user said yes.
    private(set) var allowsCellular = false
    /// A file is downloading (not being checked and installed).
    private var fetching = false

    /// A Wi-Fi-only download is held: the connection is cellular, or there
    /// is none. The card says "Waiting for Wi-Fi" and offers Use cellular.
    var waitsForWiFi: Bool { fetching && !allowsCellular && route != .wifi }

    static let dismissedKey = "setupCardDismissed"

    init(
        setup: any Setup,
        downloader: any ModelDownloader,
        network: any NetworkConditions,
        jobs: LongJobRunner,
        defaults: UserDefaults = .standard
    ) {
        self.setup = setup
        self.downloader = downloader
        self.network = network
        self.jobs = jobs
        self.defaults = defaults
        snapshot = setup.snapshot()
        cardDismissed = defaults.bool(forKey: Self.dismissedKey)
        route = network.route
        network.onChange = { [weak self] route in
            self?.route = route
        }
    }

    private(set) var cardDismissed: Bool

    /// Home's card sets up name finding. Below the floor there is none: the
    /// text-recognition download waits in the document that needs it, and
    /// in Settings (ADR 0088).
    var showsCard: Bool { snapshot.tier == .detector && snapshot.needsSetup && !cardDismissed }

    var physicalMemory: UInt64 { snapshot.physicalMemory }

    func dismissCard() {
        cardDismissed = true
        defaults.set(true, forKey: Self.dismissedKey)
    }

    func state(of offer: ModelOffer) -> ModelState {
        if let fraction = downloadProgress[offer.id] {
            return waitsForWiFi ? .waitingForWiFi : .downloading(fraction: fraction)
        }
        return offer.state
    }

    /// Download pressed: on cellular ask first, else go on Wi-Fi.
    /// `then` runs once every offered model is installed: the document card
    /// passes the scan that asked for setup (Download & continue, ADR 0026);
    /// Settings passes nothing.
    func download(then continuation: (@MainActor () async -> Void)? = nil) {
        guard !isWorking else { return }
        self.continuation = continuation
        if route == .cellular {
            asksCellular = true
            return
        }
        begin(allowCellular: false)
    }

    /// Use cellular, while a download waits for Wi-Fi: the same question.
    func askCellular() {
        guard waitsForWiFi else { return }
        asksCellular = true
    }

    /// The answer to the cellular question. Waiting keeps the download for
    /// Wi-Fi; a yes while one waits starts it again on the cellular session.
    func answerCellular(useCellular: Bool) {
        asksCellular = false
        guard isWorking else {
            begin(allowCellular: useCellular)
            return
        }
        guard useCellular, !allowsCellular else { return }
        let held = work
        held?.cancel()
        begin(allowCellular: true, after: held)
    }

    /// Stops the download or the install; the models return to not
    /// downloaded, ready to try again.
    func cancel() {
        guard isWorking else { return }
        continuation = nil
        work?.cancel()
        if jobs.current?.kind == .install {
            jobs.cancel()
        }
    }

    /// Waits for the download started last (tests).
    func waitForDownload() async {
        await work?.value
    }

    func refresh() {
        snapshot = setup.snapshot()
    }

    private var work: Task<Void, Never>?
    private var continuation: (@MainActor () async -> Void)?
    /// The download started last; one it replaced ends quietly.
    private var generation = 0

    /// `held`, a Wi-Fi download being cancelled for cellular, ends first.
    private func begin(allowCellular: Bool, after held: Task<Void, Never>? = nil) {
        isWorking = true
        allowsCellular = allowCellular
        errorMessage = nil
        generation += 1
        let generation = generation
        work = Task { [weak self] in
            await held?.value
            await self?.run(allowCellular: allowCellular, generation: generation)
        }
    }

    private func run(allowCellular: Bool, generation: Int) async {
        let finished = await install(allowCellular: allowCellular)
        fetching = false
        downloadProgress = [:]
        refresh()
        // Replaced by the cellular download: it goes on, with the work that
        // asked for setup.
        guard generation == self.generation else { return }
        isWorking = false
        allowsCellular = false
        let next = continuation
        continuation = nil
        if finished && !snapshot.needsSetup {
            await next?()
        }
    }

    /// True when every missing model was installed: each pinned file is
    /// downloaded by the platform, then verified and installed by the core.
    private func install(allowCellular: Bool) async -> Bool {
        for offer in snapshot.missing {
            do {
                let requests = try setup.downloadRequests(for: offer.id)
                let total = max(requests.reduce(0) { $0 + $1.byteSize }, 1)
                var done: Int64 = 0
                downloadProgress[offer.id] = 0
                for request in requests {
                    let before = done
                    fetching = true
                    let file = try await downloader.download(request, allowCellular: allowCellular) { fraction in
                        Task { @MainActor [weak self] in
                            if self?.downloadProgress[offer.id] != nil {
                                self?.downloadProgress[offer.id] = (Double(before) + fraction * Double(request.byteSize)) / Double(total)
                            }
                        }
                    }
                    // A failed download returns through `run`, which resets it.
                    fetching = false
                    done += request.byteSize
                    try Task.checkCancellation()
                    let setup = self.setup
                    let outcome = await jobs.run(.install, documentID: nil, title: String(localized: "Installing \(offer.name)")) { _ in
                        try setup.install(request, from: file)
                    }
                    switch outcome {
                    case .finished:
                        break
                    case .cancelled:
                        return false
                    case .busy:
                        errorMessage = String(localized: "Another job is running. Try again when it finishes.")
                        return false
                    case .failed(let error):
                        // A refused file is downloaded again next time.
                        try? FileManager.default.removeItem(at: file)
                        errorMessage = error.localizedDescription
                        return false
                    }
                }
                downloadProgress[offer.id] = nil
            } catch is CancellationError {
                return false
            } catch {
                errorMessage = error.localizedDescription
                return false
            }
        }
        return true
    }
}
