import Foundation

/// How this launch runs, read once from the process's arguments and
/// environment: every UI-test flag in one place (ADR 0104). A normal launch
/// has none of them.
struct LaunchConfiguration: Sendable, Equatable {
    /// Launch argument for UI tests: a separate, fresh storage root, case
    /// key and preferences, and a device below the detector floor, so no
    /// model is needed; the seeded document is scanned with the structured
    /// rules.
    static let uiTestingArgument = "-mdoc-ui-testing"

    /// `-mdoc-ui-testing`.
    var isUITesting = false
    /// `MDOC_SEED_FILE`: the fixture a UI test imports at launch (a file, a
    /// Markdown file as pasted text, or a folder of JPEG pages as a scan).
    var seedFile: String?
    /// `MDOC_REAL_DOWNLOADS=1`: UI tests that need a model (OCR) download it
    /// for real; the others use a downloader that fetches nothing.
    var realDownloads = false
    /// `MDOC_UI_DETECTOR_TIER=1`: the UI survey and the device long-jobs
    /// test run at the detector tier instead of below the floor.
    var detectorTier = false
    /// `MDOC_SLOW_TOASTS=1`: the UI survey: a toast stays long enough to
    /// photograph, its countdown still, so its screen settles.
    var slowToasts = false
    /// `MDOC_KEEP_LIBRARY=1`: a relaunch in a UI test opens the test library
    /// as the last launch left it, to check what was saved.
    var keepsLibrary = false
    /// `MDOC_UI_RULES_FIND_NAMES=1`: below the floor, Find names and Find
    /// names again show as at the detector tier and run the structured
    /// rules in the detector's place, so a UI test finds an email it typed
    /// without the model (ADR 0109). The app never offers this tier.
    var rulesFindNames = false

    init() {}

    init(arguments: [String], environment: [String: String]) {
        isUITesting = arguments.contains(Self.uiTestingArgument)
        guard isUITesting else { return }
        seedFile = environment["MDOC_SEED_FILE"]
        slowToasts = environment["MDOC_SLOW_TOASTS"] == "1"
        realDownloads = environment["MDOC_REAL_DOWNLOADS"] == "1"
        detectorTier = environment["MDOC_UI_DETECTOR_TIER"] == "1"
        keepsLibrary = environment["MDOC_KEEP_LIBRARY"] == "1"
        rulesFindNames = environment["MDOC_UI_RULES_FIND_NAMES"] == "1"
    }

    /// This process's launch, parsed on first use.
    static let current = LaunchConfiguration(
        arguments: ProcessInfo.processInfo.arguments, environment: ProcessInfo.processInfo.environment)

    /// Below the detector floor: the app offers no Find names, and only the
    /// UI tests' seed scans, with the structured rules.
    static let belowFloorMemory: UInt64 = 4 * 1_073_741_824

    /// The RAM the core plans models for: UI tests stand below the floor
    /// unless they ask for the detector tier.
    var physicalMemory: UInt64 {
        isUITesting && !detectorTier ? Self.belowFloorMemory : ProcessInfo.processInfo.physicalMemory
    }
}
