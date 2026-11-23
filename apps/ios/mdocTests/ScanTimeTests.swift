import MdocFFI
import XCTest
@testable import mdoc

/// A phone's scan as the bar shows it (ADR 0120): the time left, the pause
/// to cool down, the estimate before a long scan and the speed in Settings.
/// The core's own tests cover the stall guard, the free-memory check, the
/// ceiling and when to pause.
@MainActor
final class ScanTimeTests: XCTestCase {
    func testTheBarShowsTimeLeftAndKeepsItThroughACoolDown() {
        let observer = TestObserver()
        let reporter = StageReporter(observer)
        reporter.stage(stage: .scanning)
        reporter.advanced(done: 1, total: 4)
        reporter.remaining(seconds: 200)
        reporter.stage(stage: .coolingDown)
        reporter.stage(stage: .scanning)
        let updates = observer.updates
        XCTAssertEqual(updates.map(\.stage), [.scanning, .scanning, .scanning, .coolingDown, .scanning])
        XCTAssertEqual(updates[2], JobUpdate(stage: .scanning, fraction: 0.25, secondsLeft: 200))
        XCTAssertEqual(ActionBar.progressLabel(.scan, updates[2]), "Finding names · About 3 min left")
        XCTAssertEqual(ActionBar.progressLabel(.scan, updates[3]), "Paused to cool down")
        XCTAssertEqual(updates[3].fraction, 0.25, "the bar stays where it was while paused")
        XCTAssertEqual(updates[4], updates[2], "back from cooling down, the bar keeps where it was")
    }

    func testTheCoreHearsThePhonesHeatAndFreeMemory() {
        let reporter = StageReporter(
            TestObserver(), device: DeviceConditions(thermal: { .critical }, freeMemory: { 1_000 }))
        XCTAssertEqual(reporter.thermal(), .critical)
        XCTAssertEqual(reporter.freeMemory(), 1_000)
        // The Simulator has no memory limit, so it reports none.
        XCTAssertNil(DeviceConditions.live.freeMemory())
    }

    func testTimeReadsInWholeMinutesOrSecondsUnderOne() {
        XCTAssertEqual(ScanTime.left(seconds: 45), "Less than a minute left")
        XCTAssertEqual(ScanTime.left(seconds: 100), "About 2 min left")
        XCTAssertEqual(ScanTime.perTenPages(seconds: 62), "About 62 s per 10 pages")
        XCTAssertEqual(ScanTime.perTenPages(seconds: 125), "About 2 min per 10 pages")
        XCTAssertEqual(ScanTime.estimate(minutes: 3), "About 3 min on this iPhone")
        XCTAssertEqual(ActionBar.progressLabel(.scan, JobUpdate(stage: .scanning)), "Finding names")
    }

    func testBelowTheFloorSaysWhy() {
        XCTAssertTrue(BelowFloor.note(.memory).contains("4\u{00A0}GB of memory"))
        XCTAssertTrue(BelowFloor.note(.speed).contains("too slow"))
    }

    /// The estimate shows under Find names, and not once the scan runs.
    func testTheEstimateShowsBeforeTheScanOnly() async throws {
        let (core, storage) = try Fixture.core()
        let real = try core.library.importDocument(.text("Ann Lee signed.\n"), title: "Note", into: nil)
        _ = try real.recognize(observer: TestObserver())
        let session = SlowJobs(real, seconds: 0.3)
        session.estimate = 3
        let jobs = LongJobRunner(background: NoBackgroundContinuation())
        let model = DocumentViewModel(session: session, jobs: jobs, storage: storage, findsNames: true)
        XCTAssertEqual(model.primaryAction, .findNames)
        XCTAssertEqual(model.scanEstimate, 3)
        let scan = Task { await model.findNames() }
        while model.runningJob == nil { await Task.yield() }
        XCTAssertNil(model.scanEstimate, "the running bar shows the time left instead")
        await scan.value
        let below = DocumentViewModel(session: session, jobs: jobs, storage: storage, findsNames: false)
        XCTAssertNil(below.scanEstimate, "below the floor there is no Find names")
    }

    /// The detector is timed once it installs. A phone found too slow does
    /// not run the scan that asked for setup, and its open documents stop
    /// offering Find names rather than scanning with the rules alone.
    func testAPhoneTimedTooSlowStopsFindingNames() async throws {
        let setup = FakeSetup(memory: Fixture.detectorTier)
        setup.timesTooSlow = true
        let model = SetupViewModel(
            setup: setup, downloader: FakeModelDownloader(seconds: 0), network: FakeNetwork(.wifi),
            jobs: LongJobRunner(background: NoBackgroundContinuation()), preferences: Fixture.preferences())
        let (core, storage) = try Fixture.core()
        let real = try core.library.importDocument(.text("Ann Lee signed.\n"), title: "Note", into: nil)
        _ = try real.recognize(observer: TestObserver())
        let document = DocumentViewModel(
            session: real, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage,
            canFindNames: { model.snapshot.tier == .detector })
        XCTAssertTrue(document.findsNames)
        var continued = false
        model.download { continued = true }
        await model.waitForDownload()
        XCTAssertEqual(model.snapshot.belowFloor, .speed)
        XCTAssertFalse(continued, "the scan that asked for setup does not run")
        XCTAssertFalse(document.findsNames)
        XCTAssertTrue(document.showsBelowFloorNote)
    }

    /// The lists' "Ready to mark names" follows the tier too: a phone timed
    /// too slow after launch stops listing documents as ready to find names.
    func testTheLibraryFollowsATierTimedAfterLaunch() async throws {
        let setup = FakeSetup(memory: Fixture.detectorTier)
        setup.timesTooSlow = true
        let (core, storage) = try Fixture.core()
        let app = AppModel(
            core: Core(library: core.library, setup: setup), storage: storage,
            background: NoBackgroundContinuation(), downloader: FakeModelDownloader(seconds: 0),
            network: FakeNetwork(.wifi), preferences: Fixture.preferences())
        XCTAssertTrue(app.library.findsNames)
        app.setup.download()
        await app.setup.waitForDownload()
        XCTAssertEqual(app.setup.snapshot.belowFloor, .speed)
        XCTAssertFalse(app.library.findsNames)
    }

    /// A phone timed within the ceiling keeps its speed for Settings.
    func testATimedPhoneShowsItsSpeed() async {
        let model = SetupViewModel(
            setup: FakeSetup(memory: Fixture.detectorTier), downloader: FakeModelDownloader(seconds: 0),
            network: FakeNetwork(.wifi), jobs: LongJobRunner(background: NoBackgroundContinuation()),
            preferences: Fixture.preferences())
        var continued = false
        model.download { continued = true }
        await model.waitForDownload()
        XCTAssertEqual(model.snapshot.tenPagesSeconds, 62)
        XCTAssertTrue(continued)
    }

    /// A detector-tier phone that is not timed yet shows no estimate.
    func testNoEstimateBeforeThePhoneIsTimed() throws {
        let (core, _) = try Fixture.core(memory: Fixture.detectorTier)
        let session = try core.library.importDocument(.text("Ann Lee signed.\n"), title: "Note", into: nil)
        _ = try session.recognize(observer: TestObserver())
        XCTAssertNil(session.scanEstimate())
        XCTAssertNil(core.setup.snapshot().tenPagesSeconds)
        XCTAssertNil(core.setup.snapshot().belowFloor)
        XCTAssertEqual(
            try core.setup.timeDetector(observer: TestObserver()).tier, .detector, "with no detector installed nothing is timed")
    }
}
