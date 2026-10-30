import Foundation
import Observation

/// The first-use setup card and Settings' model rows: the models that fit
/// this device by physical RAM, their size, and the download (ADR 0026,
/// ADR 0085). Wi-Fi by default; cellular asks first.
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
    }

    private(set) var cardDismissed: Bool

    var showsCard: Bool { snapshot.needsSetup && !cardDismissed }

    var physicalMemory: UInt64 { snapshot.physicalMemory }

    func dismissCard() {
        cardDismissed = true
        defaults.set(true, forKey: Self.dismissedKey)
    }

    func state(of offer: ModelOffer) -> ModelState {
        if let fraction = downloadProgress[offer.id] {
            return .downloading(fraction: fraction)
        }
        return offer.state
    }

    /// Download pressed: on a costly connection ask first, else go on Wi-Fi.
    /// `then` runs once every offered model is installed: the document card
    /// passes the scan that asked for setup (Download & continue, ADR 0026);
    /// Settings passes nothing.
    func download(then continuation: (@MainActor () async -> Void)? = nil) {
        guard !isWorking else { return }
        self.continuation = continuation
        if network.isExpensive {
            asksCellular = true
            return
        }
        begin(allowCellular: false)
    }

    /// The answer to the cellular question. Waiting keeps the download for
    /// Wi-Fi.
    func answerCellular(useCellular: Bool) {
        asksCellular = false
        begin(allowCellular: useCellular)
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

    private func begin(allowCellular: Bool) {
        isWorking = true
        errorMessage = nil
        work = Task { [weak self] in
            await self?.run(allowCellular: allowCellular)
        }
    }

    private func run(allowCellular: Bool) async {
        let finished = await install(allowCellular: allowCellular)
        isWorking = false
        downloadProgress = [:]
        refresh()
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
                    let file = try await downloader.download(request, allowCellular: allowCellular) { fraction in
                        Task { @MainActor [weak self] in
                            if self?.downloadProgress[offer.id] != nil {
                                self?.downloadProgress[offer.id] = (Double(before) + fraction * Double(request.byteSize)) / Double(total)
                            }
                        }
                    }
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
