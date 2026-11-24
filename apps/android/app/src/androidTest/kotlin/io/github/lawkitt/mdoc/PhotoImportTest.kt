package io.github.lawkitt.mdoc

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.media.ExifInterface
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.system.Os
import android.system.OsConstants
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.CameraCapture
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.OriginalData
import io.github.lawkitt.mdoc.core.OriginalSource
import io.github.lawkitt.mdoc.core.Originals
import io.github.lawkitt.mdoc.core.ScanSession
import io.github.lawkitt.mdoc.core.SourceKind
import io.github.lawkitt.mdoc.home.HomeViewModel
import io.github.lawkitt.mdoc.home.PendingAdd
import io.github.lawkitt.mdoc.storage.MdocStorage
import io.github.lawkitt.mdoc.ui.MdocTheme
import io.github.lawkitt.mdoc.ui.OriginalDecodes
import io.github.lawkitt.mdoc.ui.OriginalView
import io.github.lawkitt.mdoc.ui.PAGE_PLACEHOLDER
import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.UUID
import kotlin.math.abs
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PhotoImportTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<ComponentActivity>()

    /** Each test's scan starts without another test's record. */
    @Before
    fun noScanRecord() {
        File(CameraCapture.dir(core.app).parentFile, "scan.json").delete()
    }

    private val resolver get() = core.app.contentResolver
    private val home get() = core.app.core.let { isolated -> HomeViewModel({ isolated }) }
    private val originals get() = core.storage.dir(MdocStorage.Area.Originals)

    @Test
    fun theAddSheetOnlyStagesSanitizedPhotosAndCancelDeletesThem() = runBlocking {
        val home = home
        val photo = CameraCapture.create(core.app)
        try {
            writePhoto(photo, taggedJpeg(6))
            assertNotNull(resolver.openInputStream(photo)!!.use { ExifInterface(it).getAttribute(ExifInterface.TAG_GPS_LATITUDE) })
            val add = receive(home, listOf(photo))
            val page = (add.source as ImportSource.Pages).pages.single()
            assertClean(page)
            assertEquals(6, ExifInterface(page).getAttributeInt(ExifInterface.TAG_ORIENTATION, 0))
            withContext(Dispatchers.Main) { home.cancelAdd() }
            assertNull(home.state.value.adding)
            assertTrue(originals.listFiles()!!.isEmpty())
        } finally {
            discard(photo)
        }
    }

    @Test
    fun aFailedSelectionAndFailedAddLeaveNoStagedCopies() = runBlocking {
        val home = home
        val photo = CameraCapture.create(core.app)
        val bad = CameraCapture.create(core.app)
        try {
            writePhoto(photo, taggedJpeg(1))
            writePhoto(bad, "Unrecognized image".toByteArray())
            withContext(Dispatchers.Main) { home.received(resolver, listOf(photo, bad), SourceKind.Image, "Photo") }
            withTimeout(15_000) { home.state.first { it.refused != null } }
            assertTrue(originals.listFiles()!!.isEmpty())
            val staged = receive(home, listOf(photo))
            assertTrue(core.storage.pendingFile.mkdir())
            val id = withContext(Dispatchers.Main) { home.add(null, "Photo") }
            assertNull(id)
            assertFalse(staged.folder.exists())
        } finally {
            discard(photo)
            discard(bad)
        }
    }

    @Test
    fun extensionlessPhotosKeepTheirContentTypeAndOwnedPagesSurviveRestart() = runBlocking {
        val home = home
        val jpeg = CameraCapture.create(core.app)
        val share = File(core.app.cacheDir, "hand-off").apply { mkdirs() }
        val png = File(share, UUID.randomUUID().toString())
        try {
            writePhoto(jpeg, taggedJpeg(1))
            val bitmap = Bitmap.createBitmap(40, 30, Bitmap.Config.ARGB_8888)
            try {
                bitmap.setPixel(7, 10, Color.BLUE)
                assertTrue(png.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) })
            } finally {
                bitmap.recycle()
            }
            val extensionless = FileProvider.getUriForFile(core.app, "${core.app.packageName}.files", png)
            val staged = receive(home, listOf(jpeg, extensionless))
            val pages = (staged.source as ImportSource.Pages).pages
            assertEquals(listOf("jpg", "png"), pages.map { it.extension })
            val kept = checkNotNull(BitmapFactory.decodeFile(pages.last().path))
            try {
                assertEquals(40, kept.width)
                assertEquals(30, kept.height)
                assertEquals(Color.BLUE, kept.getPixel(7, 10))
                assertEquals(Color.TRANSPARENT, kept.getPixel(0, 0))
            } finally {
                kept.recycle()
            }
            val id = withContext(Dispatchers.Main) { home.add(null, "Photos") }!!
            val abandoned = Originals(core.storage).run {
                newStage().also { stageImage(File(it, "abandoned.jpg"), taggedJpeg(1)) }
            }
            core.app.reopenCore()
            val reopened = core.app.core.documents.session(id).snapshot()
            assertEquals(OriginalSource.Images(pages.map(OriginalData::Local)), reopened.original)
            assertFalse(abandoned.exists())
            assertTrue(staged.folder.isDirectory)
        } finally {
            discard(jpeg)
            png.delete()
        }
    }

    @Test
    fun aCameraShotIsDeletedOnceItsCleanCopyIsKept() = runBlocking {
        val home = home
        val uri = withContext(Dispatchers.Main) { home.takePage(core.app) }!!
        // This is the seekable output the camera contract gives to another app.
        writePhoto(uri, taggedJpeg(6))
        assertTrue(originals.listFiles()!!.isEmpty())
        withContext(Dispatchers.Main) { home.cameraResult(true) }
        withTimeout(15_000) { home.state.first { it.scanPageCount == 1 } }
        // The scan records the page before it deletes the raw shot, so a
        // crash between the two never loses it (#1100).
        compose.waitUntil(15_000) { CameraCapture.file(core.app, uri) == null }
        assertTrue(runCatching { resolver.openInputStream(uri)?.close() }.isFailure)
        withContext(Dispatchers.Main) { home.finishScan("Scan") }
        val staged = home.state.value.adding as PendingAdd.Received
        assertClean((staged.source as ImportSource.Pages).pages.single())
        withContext(Dispatchers.Main) { home.cancelAdd() }
        assertTrue(originals.listFiles()!!.isEmpty())

        val cancelled = withContext(Dispatchers.Main) { home.takePage(core.app) }!!
        writePhoto(cancelled, taggedJpeg(6))
        withContext(Dispatchers.Main) { home.cameraResult(false) }
        assertTrue(runCatching { resolver.openInputStream(cancelled)?.close() }.isFailure)
        assertTrue(originals.listFiles()!!.isEmpty())
    }

    @Test
    fun aFailedEncodeOrRenameDoesNotReplaceTheStagedPage() {
        val originals = Originals(core.storage)
        val folder = originals.newStage()
        val page = File(folder, "page.jpg").apply { writeText("complete page") }
        assertThrows(IllegalStateException::class.java) {
            Originals.writeAtomically(page) {
                it.write("partial encode".toByteArray())
                false
            }
        }
        assertEquals("complete page", page.readText())
        assertEquals(listOf(page), folder.listFiles()!!.toList())

        val destination = File(folder, "other.jpg").apply { mkdir() }
        val existing = File(destination, "existing").apply { writeText("existing") }
        assertThrows(java.io.IOException::class.java) {
            originals.stageImage(destination, taggedJpeg(1))
        }
        assertEquals("existing", existing.readText())
        assertEquals(setOf(page, destination), folder.listFiles()!!.toSet())
    }

    @Test
    fun originalDrawsAsymmetricPhotosUprightForEveryExifOrientation() {
        val originals = Originals(core.storage)
        val folder = originals.newStage()
        val pages = (1..8).map { orientation ->
            originals.stageImage(File(folder, "page-$orientation"), taggedJpeg(orientation))
        }
        var page by mutableStateOf(pages.first())
        compose.setContent {
            MdocTheme {
                Box(Modifier.size(160.dp, 120.dp)) {
                    key(page) { OriginalView(OriginalSource.Images(listOf(OriginalData.Local(page))), Modifier.fillMaxSize()) }
                }
            }
        }
        val description = "Original page 1 of 1"
        pages.forEachIndexed { index, file ->
            compose.runOnIdle { page = file }
            compose.waitUntil(10_000) {
                compose.onAllNodes(hasContentDescription(description)).fetchSemanticsNodes().isNotEmpty() &&
                    compose.onAllNodes(hasTestTag(PAGE_PLACEHOLDER)).fetchSemanticsNodes().isEmpty()
            }
            val drawn = compose.onNodeWithContentDescription(description).captureToImage().toPixelMap()
            for ((corner, expected) in corners.withIndex()) {
                val x = drawn.width * (if (corner % 2 == 0) 1 else 3) / 4
                val y = drawn.height * (if (corner / 2 == 0) 1 else 3) / 4
                val actual = drawn[x, y].toArgb()
                val distance = abs(Color.red(expected) - Color.red(actual)) +
                    abs(Color.green(expected) - Color.green(actual)) + abs(Color.blue(expected) - Color.blue(actual))
                assertTrue("EXIF ${index + 1}, corner $corner: expected $expected, drew $actual", distance < 35)
            }
            assertClean(file)
        }
    }

    /**
     * A scan outlives its screen and its process while the camera is open:
     * the activity is destroyed (Home's ViewModel cleared) with a shot in
     * flight, then the process is gone and only the saved state is left.
     * The restored scan still takes the shot, and the next start keeps the
     * scan's stage.
     */
    @Test
    fun aScanSurvivesItsScreenAndItsProcessWhileTheCameraIsOpen() = runBlocking {
        val scan = ScanSession(CameraCapture.dir(core.app))
        val saved = SavedStateHandle()
        val store = ViewModelStore()
        val factory = viewModelFactory { initializer { HomeViewModel({ core.app.core }, null, scan, saved) } }
        val first = withContext(Dispatchers.Main) { ViewModelProvider.create(store, factory)[HomeViewModel::class] }
        val page1 = withContext(Dispatchers.Main) { first.takePage(core.app) }!!
        writePhoto(page1, taggedJpeg(6))
        withContext(Dispatchers.Main) { first.cameraResult(true) }
        withTimeout(15_000) { first.state.first { it.scanPageCount == 1 } }
        // The second shot is in flight when the activity goes.
        val page2 = withContext(Dispatchers.Main) { first.takePage(core.app) }!!
        withContext(Dispatchers.Main) { store.clear() }
        val kept = scan.state.value.pages.single()
        assertTrue(kept.isFile)
        writePhoto(page2, taggedJpeg(3))
        // A new process: nothing in memory but the saved state, and a new
        // start of the library, which sweeps abandoned stages.
        core.app.reopenCore()
        core.app.core.documents
        val restoredState = SavedStateHandle(saved.keys().associateWith { saved.get<Any?>(it) })
        val restored = withContext(Dispatchers.Main) {
            HomeViewModel({ core.app.core }, null, ScanSession(CameraCapture.dir(core.app)), restoredState)
        }
        assertTrue(kept.isFile)
        withContext(Dispatchers.Main) { restored.cameraResult(true) }
        withTimeout(15_000) { restored.state.first { it.scanPageCount == 2 } }
        // The raw shot goes once its clean copy is recorded, a moment after
        // the page shows (#1100).
        compose.waitUntil(15_000) { CameraCapture.file(core.app, page2) == null }
        assertTrue(CameraCapture.dir(core.app).listFiles().orEmpty().isEmpty())
        withContext(Dispatchers.Main) { restored.finishScan("Scan") }
        val pages = ((restored.state.value.adding as PendingAdd.Received).source as ImportSource.Pages).pages
        assertEquals(2, pages.size)
        pages.forEach(::assertClean)
        withContext(Dispatchers.Main) { restored.cancelAdd() }
    }

    /**
     * The shot's cleaning is the scan's own job: Home's ViewModel destroyed
     * right after the camera returns loses no page.
     */
    @Test
    fun aShotBeingCleanedOutlivesHome() = runBlocking {
        val scan = ScanSession(CameraCapture.dir(core.app))
        val store = ViewModelStore()
        val factory = viewModelFactory { initializer { HomeViewModel({ core.app.core }, null, scan, SavedStateHandle()) } }
        val home = withContext(Dispatchers.Main) { ViewModelProvider.create(store, factory)[HomeViewModel::class] }
        val shot = withContext(Dispatchers.Main) { home.takePage(core.app) }!!
        writePhoto(shot, taggedJpeg(6))
        withContext(Dispatchers.Main) {
            home.cameraResult(true)
            store.clear()
        }
        withTimeout(15_000) { scan.state.first { it.pages.size == 1 && it.capture == null } }
        assertClean(scan.state.value.pages.single())
        // A new Home saves it, and the raw shot goes just after the page is
        // recorded (#1100).
        withContext(Dispatchers.Main) { HomeViewModel({ core.app.core }, null, scan, SavedStateHandle()) }
        compose.waitUntil(15_000) { CameraCapture.file(core.app, shot) == null }
    }

    /**
     * A stopped activity's saved state isn't written again: a page cleaned
     * after it was taken is found in the scan's own record when that
     * snapshot is restored in a new process.
     */
    @Test
    fun aPageCleanedAfterTheSnapshotIsRestored() = runBlocking {
        val scan = ScanSession(CameraCapture.dir(core.app))
        val saved = SavedStateHandle()
        val home = withContext(Dispatchers.Main) { HomeViewModel({ core.app.core }, null, scan, saved) }
        val shot = withContext(Dispatchers.Main) { home.takePage(core.app) }!!
        writePhoto(shot, taggedJpeg(6))
        // The activity stops: this is the snapshot the system keeps.
        val snapshot = withContext(Dispatchers.Main) { saved.keys().associateWith { saved.get<Any?>(it) } }
        withContext(Dispatchers.Main) { home.cameraResult(true) }
        withTimeout(15_000) { scan.state.first { it.pages.size == 1 && it.capture == null } }
        // The raw shot goes just after the page is recorded (#1100).
        compose.waitUntil(15_000) { CameraCapture.file(core.app, shot) == null }
        // A new process restores the stopped activity's snapshot.
        val restored = withContext(Dispatchers.Main) {
            HomeViewModel({ core.app.core }, null, ScanSession(CameraCapture.dir(core.app)), SavedStateHandle(snapshot))
        }
        assertEquals(1, restored.state.value.scanPageCount)
        withContext(Dispatchers.Main) { restored.finishScan("Scan") }
        val pages = ((restored.state.value.adding as PendingAdd.Received).source as ImportSource.Pages).pages
        assertEquals(1, pages.size)
        assertClean(pages.single())
        withContext(Dispatchers.Main) { restored.cancelAdd() }
    }

    /**
     * When the scan's record can't be written, the raw shot stays (outside
     * `originals/`, swept later) and Home says the page wasn't saved.
     */
    @Test
    fun aShotStaysWhenItsPageCantBeRecorded() = runBlocking {
        val scan = ScanSession(CameraCapture.dir(core.app))
        val home = withContext(Dispatchers.Main) { HomeViewModel({ core.app.core }, null, scan, SavedStateHandle()) }
        val shot = withContext(Dispatchers.Main) { home.takePage(core.app) }!!
        writePhoto(shot, taggedJpeg(1))
        val noBackup = CameraCapture.dir(core.app).parentFile!!
        try {
            assertTrue(noBackup.setWritable(false, false))
            withContext(Dispatchers.Main) { home.cameraResult(true) }
            withTimeout(15_000) { home.state.first { it.refused != null } }
            assertEquals("mdoc could not save this page. Take it again.", home.state.value.refused)
            assertNotNull(CameraCapture.file(core.app, shot))
        } finally {
            noBackup.setWritable(true, false)
            discard(shot)
        }
    }

    /**
     * A record a failed write left behind is older than the saved state
     * after it: a restore takes the saved state's newer shot and doesn't
     * sweep it.
     */
    @Test
    fun aStaleRecordDoesNotHideANewerShot() = runBlocking {
        val scan = ScanSession(CameraCapture.dir(core.app))
        val saved = SavedStateHandle()
        val home = withContext(Dispatchers.Main) { HomeViewModel({ core.app.core }, null, scan, saved) }
        val first = withContext(Dispatchers.Main) { home.takePage(core.app) }!!
        writePhoto(first, taggedJpeg(1))
        withContext(Dispatchers.Main) { home.cameraResult(true) }
        withTimeout(15_000) { scan.state.first { it.pages.size == 1 && it.capture == null } }
        val noBackup = CameraCapture.dir(core.app).parentFile!!
        val second: Uri
        val snapshot: Map<String, Any?>
        try {
            // The record keeps the first page; recording the next shot fails.
            assertTrue(noBackup.setWritable(false, false))
            second = withContext(Dispatchers.Main) { home.takePage(core.app) }!!
            writePhoto(second, taggedJpeg(3))
            snapshot = withContext(Dispatchers.Main) { saved.keys().associateWith { saved.get<Any?>(it) } }
        } finally {
            noBackup.setWritable(true, false)
        }
        val restored = withContext(Dispatchers.Main) {
            HomeViewModel({ core.app.core }, null, ScanSession(CameraCapture.dir(core.app)), SavedStateHandle(snapshot))
        }
        assertNotNull(CameraCapture.file(core.app, second))
        withContext(Dispatchers.Main) { restored.cameraResult(true) }
        withTimeout(15_000) { restored.state.first { it.scanPageCount == 2 } }
        withContext(Dispatchers.Main) {
            restored.finishScan("Scan")
            restored.cancelAdd()
        }
    }

    /**
     * A first page that can't be staged leaves no record naming a page:
     * its raw shot stays for the next start's sweep instead of going at
     * once, and Home says why.
     */
    @Test
    fun aFirstPageThatCantBeStagedKeepsItsShot() = runBlocking {
        val scan = ScanSession(CameraCapture.dir(core.app))
        val home = withContext(Dispatchers.Main) { HomeViewModel({ core.app.core }, null, scan, SavedStateHandle()) }
        val shot = withContext(Dispatchers.Main) { home.takePage(core.app) }!!
        writePhoto(shot, "Not a photo at all.".toByteArray())
        try {
            withContext(Dispatchers.Main) { home.cameraResult(true) }
            withTimeout(15_000) { home.state.first { it.refused != null } }
            assertTrue(scan.state.value.pages.isEmpty())
            assertNotNull(CameraCapture.file(core.app, shot))
        } finally {
            discard(shot)
        }
    }

    /**
     * A record newer than an empty saved snapshot (saved before the scan
     * began) is where the scan stands: the restore takes it, and doesn't
     * delete it.
     */
    @Test
    fun aNewerRecordOutlivesAnEmptySnapshot() = runBlocking {
        val scan = ScanSession(CameraCapture.dir(core.app))
        val saved = SavedStateHandle()
        val home = withContext(Dispatchers.Main) { HomeViewModel({ core.app.core }, null, scan, saved) }
        // The activity stops before the scan begins: an empty snapshot.
        val snapshot = withContext(Dispatchers.Main) { saved.keys().associateWith { saved.get<Any?>(it) } }
        val shot = withContext(Dispatchers.Main) { home.takePage(core.app) }!!
        writePhoto(shot, taggedJpeg(1))
        withContext(Dispatchers.Main) { home.cameraResult(true) }
        withTimeout(15_000) { scan.state.first { it.pages.size == 1 && it.capture == null } }
        val restored = withContext(Dispatchers.Main) {
            HomeViewModel({ core.app.core }, null, ScanSession(CameraCapture.dir(core.app)), SavedStateHandle(snapshot))
        }
        assertEquals(1, restored.state.value.scanPageCount)
        assertTrue(File(CameraCapture.dir(core.app).parentFile, "scan.json").isFile)
        withContext(Dispatchers.Main) {
            restored.finishScan("Scan")
            restored.cancelAdd()
        }
    }

    /** Shots no scan waits for go when a process starts. */
    @Test
    fun leftoverShotsAreSweptAtStart() = runBlocking {
        val leftover = CameraCapture.create(core.app)
        writePhoto(leftover, taggedJpeg(1))
        withContext(Dispatchers.Main) { HomeViewModel({ core.app.core }, null, ScanSession(CameraCapture.dir(core.app))) }
        assertNull(CameraCapture.file(core.app, leftover))
    }

    /** HEIC is drawn upright by Android as a JPEG without its GPS position or camera. */
    @Test
    fun aHeicBecomesAnUprightJpegWithoutMetadata() {
        val originals = Originals(core.storage)
        val folder = originals.newStage()
        val assets = InstrumentationRegistry.getInstrumentation().context.assets
        for (name in listOf("page-orientation-6.heic", "page-gps.heic")) {
            val page = originals.stageImage(File(folder, name), assets.open(name).use { it.readBytes() })
            assertEquals(name, "jpg", page.extension)
            assertFalse(File(folder, name).exists())
            val exif = ExifInterface(page)
            assertNull(name, exif.getAttribute(ExifInterface.TAG_GPS_LATITUDE))
            assertNull(name, exif.getAttribute(ExifInterface.TAG_MAKE))
            assertNull(name, exif.getAttribute(ExifInterface.TAG_DATETIME_ORIGINAL))
            assertTrue(name, exif.getAttributeInt(ExifInterface.TAG_ORIENTATION, 1) <= 1)
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(page.path, bounds)
            assertEquals(name, 1200 to 1600, bounds.outWidth to bounds.outHeight)
        }
    }

    /**
     * A pending scan's pages outlive a damaged pending list across two
     * restarts: while the damaged copy is kept, no stage is cleared.
     */
    @Test
    fun aPendingScanSurvivesTwoRestartsAfterADamagedList() {
        val originals = Originals(core.storage)
        val stage = originals.newStage()
        val page = originals.stageImage(File(stage, "page-1.jpg"), taggedJpeg(1))
        core.app.core.documents.importDocument(ImportSource.Pages(listOf(page), SourceKind.Scan), "Scan")
        core.storage.pendingFile.writeText("{ not json")
        repeat(2) {
            core.app.reopenCore()
            core.app.core.documents
            assertTrue("restart ${it + 1}", page.isFile)
        }
        assertTrue(File(core.storage.intake, "pending.json.corrupt").isFile)
    }

    /**
     * A damaged pending list that can't even be moved aside still clears
     * no stage: the failed read itself decides.
     */
    @Test
    fun aDamagedListThatCantBeMovedAsideClearsNothing() {
        val originals = Originals(core.storage)
        val stage = originals.newStage()
        val page = originals.stageImage(File(stage, "page-1.jpg"), taggedJpeg(1))
        core.app.core.documents.importDocument(ImportSource.Pages(listOf(page), SourceKind.Scan), "Scan")
        core.storage.pendingFile.writeText("{ not json")
        // The quarantine's place is taken by a folder that can't be replaced.
        File(core.storage.intake, "pending.json.corrupt").apply { mkdirs() }.resolve("keep").writeText("x")
        core.app.reopenCore()
        core.app.core.documents
        assertTrue(page.isFile)
        assertTrue(core.storage.pendingFile.isFile)
    }

    /** A damaged pending list is set aside: imports go on, and no stage is cleared on its word. */
    @Test
    fun aDamagedPendingListIsSetAside() {
        val pendingFile = core.storage.pendingFile.apply { writeText("{ not json") }
        val stage = Originals(core.storage).newStage()
        core.app.reopenCore()
        val session = core.app.core.documents.importDocument(ImportSource.Text("A short note."), "Note")
        assertNotNull(session)
        assertTrue(File(core.storage.intake, "pending.json.corrupt").isFile)
        assertTrue(pendingFile.isFile)
        assertTrue(stage.isDirectory)
    }

    private fun discard(uri: Uri) {
        CameraCapture.file(core.app, uri)?.delete()
    }

    /** Twenty photos: the list lays out and decodes only what it shows, not every page at once. */
    @Test
    fun aLongScanDecodesOnlyTheVisiblePages() {
        val originals = Originals(core.storage)
        val folder = originals.newStage()
        val pages = (1..20).map { originals.stageImage(File(folder, "page-$it"), taggedJpeg(1)) }
        val before = OriginalDecodes.started.get()
        compose.setContent {
            MdocTheme {
                Box(Modifier.size(160.dp, 240.dp)) { OriginalView(OriginalSource.Images(pages.map(OriginalData::Local)), Modifier.fillMaxSize()) }
            }
        }
        compose.waitForIdle()
        compose.waitUntil(10_000) { OriginalDecodes.started.get() > before }
        compose.waitForIdle()
        val started = OriginalDecodes.started.get() - before
        assertTrue("$started decodes", started in 1..6)
    }

    private suspend fun receive(home: HomeViewModel, uris: List<Uri>): PendingAdd.Received {
        withContext(Dispatchers.Main) { home.received(resolver, uris, SourceKind.Image, "Photo") }
        return withTimeout(15_000) { home.state.first { it.adding != null }.adding as PendingAdd.Received }
    }

    private fun writePhoto(uri: Uri, bytes: ByteArray) {
        val output = ParcelFileDescriptor.AutoCloseOutputStream(resolver.openFileDescriptor(uri, "rw")!!)
        output.use {
            it.write(bytes)
            Os.lseek(it.fd, 0, OsConstants.SEEK_SET)
            it.write(bytes, 0, 2)
        }
    }

    private fun assertClean(file: File) {
        val exif = ExifInterface(file)
        assertNull(exif.getAttribute(ExifInterface.TAG_MAKE))
        assertNull(exif.getAttribute(ExifInterface.TAG_GPS_LATITUDE))
    }

    private val corners = intArrayOf(Color.RED, Color.GREEN, Color.BLUE, Color.YELLOW)

    private fun taggedJpeg(orientation: Int): ByteArray {
        // Arrange the stored corners as the inverse of each EXIF transform,
        // so the upright picture is always the same landscape rectangle.
        val order = arrayOf(
            intArrayOf(0, 1, 2, 3), intArrayOf(1, 0, 3, 2), intArrayOf(3, 2, 1, 0), intArrayOf(2, 3, 0, 1),
            intArrayOf(0, 2, 1, 3), intArrayOf(1, 3, 0, 2), intArrayOf(3, 1, 2, 0), intArrayOf(2, 0, 3, 1),
        )[orientation - 1]
        val width = if (orientation <= 4) 80 else 60
        val height = if (orientation <= 4) 60 else 80
        val pixels = IntArray(width * height) { i ->
            val column = if (i % width < width / 2) 0 else 1
            val row = if (i / width < height / 2) 0 else 2
            corners[order[row + column]]
        }
        val bitmap = Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
        val jpeg = ByteArrayOutputStream().use { raw ->
            try {
                assertTrue(bitmap.compress(Bitmap.CompressFormat.JPEG, 100, raw))
                raw.toByteArray()
            } finally {
                bitmap.recycle()
            }
        }
        // Little-endian TIFF with orientation, a fictional camera and latitude.
        val tiff = ByteBuffer.allocate(112).order(ByteOrder.LITTLE_ENDIAN).apply {
            put("II".toByteArray()).putShort(42).putInt(8).putShort(3)
            putShort(0x0112).putShort(3).putInt(1).putInt(orientation)
            putShort(0x010f).putShort(2).putInt(8).putInt(50)
            putShort(0x8825.toShort()).putShort(4).putInt(1).putInt(58)
            putInt(0).put("Fixture\u0000".toByteArray()).putShort(2)
            putShort(1).putShort(2).putInt(2).putInt('N'.code)
            putShort(2).putShort(5).putInt(3).putInt(88)
            putInt(0).putInt(12).putInt(1).putInt(20).putInt(1).putInt(44).putInt(1)
        }.array()
        val exif = "Exif\u0000\u0000".toByteArray() + tiff
        val segment = ByteBuffer.allocate(4).put(0xff.toByte()).put(0xe1.toByte()).putShort((exif.size + 2).toShort()).array()
        return jpeg.copyOfRange(0, 2) + segment + exif + jpeg.copyOfRange(2, jpeg.size)
    }
}
