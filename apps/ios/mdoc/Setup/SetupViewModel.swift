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
    let physicalMemory: UInt64

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
        physicalMemory: UInt64 = ProcessInfo.processInfo.physicalMemory,
        defaults: UserDefaults = .standard
    ) {
        self.setup = setup
        self.downloader = downloader
        self.network = network
        self.jobs = jobs
        self.physicalMemory = physicalMemory
        self.defaults = defaults
        snapshot = setup.snapshot(physicalMemory: physicalMemory)
        cardDismissed = defaults.bool(forKey: Self.dismissedKey)
    }

    private(set) var cardDismissed: Bool

    var showsCard: Bool { snapshot.needsSetup && !cardDismissed }

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
    func download() async {
        guard !isWorking else { return }
        if network.isExpensive {
            asksCellular = true
            return
        }
        await start(allowCellular: false)
    }

    /// The answer to the cellular question. Waiting keeps the download for
    /// Wi-Fi.
    func answerCellular(useCellular: Bool) async {
        asksCellular = false
        await start(allowCellular: useCellular)
    }

    func refresh() {
        snapshot = setup.snapshot(physicalMemory: physicalMemory)
    }

    private func start(allowCellular: Bool) async {
        isWorking = true
        errorMessage = nil
        defer {
            isWorking = false
            refresh()
        }
        for offer in snapshot.missing {
            do {
                let request = try setup.downloadRequest(for: offer.id)
                downloadProgress[offer.id] = 0
                let file = try await downloader.download(request, allowCellular: allowCellular) { fraction in
                    Task { @MainActor [weak self] in
                        if self?.downloadProgress[offer.id] != nil {
                            self?.downloadProgress[offer.id] = fraction
                        }
                    }
                }
                downloadProgress[offer.id] = nil
                let setup = self.setup
                let outcome = await jobs.run(.install, documentID: nil, title: String(localized: "Installing \(offer.name)")) { observer in
                    try setup.install(model: offer.id, from: file, observer: observer)
                }
                switch outcome {
                case .finished, .cancelled:
                    break
                case .busy:
                    errorMessage = String(localized: "Another job is running. Try again when it finishes.")
                    return
                case .failed(let error):
                    errorMessage = error.localizedDescription
                    return
                }
            } catch {
                downloadProgress[offer.id] = nil
                errorMessage = error.localizedDescription
                return
            }
        }
    }
}
