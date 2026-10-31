package io.github.lawkitt.mdoc.longjobs

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.pdf.PdfDocument
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.BySelector
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.UiObject2
import androidx.test.uiautomator.Until
import java.io.File
import java.util.regex.Pattern
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.BeforeClass
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters

/**
 * Long jobs on a phone (ADR 0089, roadmap Android 0.1 item 5): the model
 * download, text recognition of a 120-page scan and Find names each go on
 * while mdoc is off screen, and each resumes after Android kills mdoc's
 * process mid-job. At the end no crash or ANR is in mdoc's
 * ApplicationExitInfo. Driven through the debug app's own screens; Find
 * names runs only on a phone above the detector floor.
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class LongJobsTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val device get() = UiDevice.getInstance(instrumentation)
    private val self get() = instrumentation.context

    @Test
    fun a1_downloadGoesOnOffScreenAndResumesAfterAKill() {
        openSettings()
        val download = device.wait(Until.findObject(By.textStartsWith("Download ")), 30_000)
        assumeTrue("the models are already installed", download != null)
        download!!.click()
        need(By.textContains("Downloading"), 60_000)
        val before = dataKb()
        device.pressHome()
        SystemClock.sleep(20_000)
        assertTrue("mdoc kept running off screen", alive())
        // Below the floor only text recognition downloads (18 MB): it can end
        // before there is anything to interrupt. Above it the detector follows.
        assumeTrue("the download finished before it could be interrupted", jobRunning())
        assertTrue("the download went on off screen", dataKb() > before)

        kill()
        openSettings()
        // Resumed where it stopped (a Range request), from the same card.
        device.wait(Until.findObject(By.textStartsWith("Download ")), 30_000)?.click()
        modelsInstalled()
    }

    @Test
    fun b1_recognitionFinishesOffScreen() {
        installModels()
        val title = share(longScan())
        recognizing()
        device.pressHome()
        awayWhileTheJobRuns()
        openDocument(title)
        recognized()
        recognizedTitle = title
    }

    @Test
    fun b2_recognitionResumesAfterAKill() {
        installModels()
        val title = share(longScan())
        recognizing()
        // Early: on a fast emulator the whole scan takes seconds.
        SystemClock.sleep(3_000)
        device.pressHome()
        kill()
        openDocument(title)
        need(By.textStartsWith("Recognition stopped when mdoc left the screen"), 30_000)
        need(By.text(Pattern.compile("Resume recognition|Finish import")), 10_000).click()
        recognized()
        resumedTitle = title
    }

    @Test
    fun c1_findNamesFinishesOffScreen() {
        val title = recognizedTitle ?: return assumeTrue("needs b1's document", false)
        openDocument(title)
        val find = device.wait(Until.findObject(By.text("Find names")), 20_000)
        assumeTrue("this phone is below the detector floor", find != null)
        find!!.click()
        need(By.text("Finding names"), 30_000)
        device.pressHome()
        awayWhileTheJobRuns()
        openDocument(title)
        namesFound()
    }

    @Test
    fun c2_findNamesStartsAgainAfterAKill() {
        val title = resumedTitle ?: return assumeTrue("needs b2's document", false)
        openDocument(title)
        val find = device.wait(Until.findObject(By.text("Find names")), 20_000)
        assumeTrue("this phone is below the detector floor", find != null)
        find!!.click()
        need(By.text("Finding names"), 30_000)
        SystemClock.sleep(10_000)
        device.pressHome()
        kill()
        openDocument(title)
        need(By.textStartsWith("Finding names stopped when mdoc left the screen"), 30_000)
        need(By.text("Find names"), 10_000).click()
        namesFound()
    }

    @Test
    fun z_noCrashOrAnr() {
        val exits = shell("dumpsys activity exit-info $MDOC")
        Log.i(TAG, exits)
        assertEquals("crashes or ANRs in mdoc's exit info:\n$exits", failuresBefore, failures(exits))
    }

    // Driving mdoc

    /** As the launcher opens it: the task as it was left, or a fresh start after a kill. */
    private fun openMdoc() {
        // From the shell: a test process in the background may not start
        // activities. Again if a start lands while Android clears a killed task.
        val front = (1..3).any {
            shell("am start -a android.intent.action.MAIN -c android.intent.category.LAUNCHER -f 0x10200000 -n $MDOC/.MainActivity")
            device.wait(Until.hasObject(By.pkg(MDOC).depth(0)), 15_000)
        }
        assertTrue("mdoc came to the front", front)
        device.waitForIdle()
    }

    /** mdoc's Home, from wherever it was left. */
    private fun home() {
        openMdoc()
        repeat(4) { if (!device.hasObject(By.desc("Settings"))) device.pressBack() }
        need(By.desc("Settings"), 20_000)
    }

    /** Opens the document from Home, or finds it already open. */
    private fun openDocument(title: String) {
        openMdoc()
        if (!device.hasObject(By.text(title))) home()
        need(By.text(title), 30_000).click()
        device.waitForIdle()
        SystemClock.sleep(1_000)
    }

    /** Settings, from Home: its setup card downloads what this phone runs. */
    private fun openSettings() {
        home()
        need(By.desc("Settings"), 20_000).click()
        need(By.text("Models"), 20_000)
    }

    /** Text recognition and, above the floor, the name detector, from Settings if they are missing. */
    private fun installModels() {
        openSettings()
        device.wait(Until.findObject(By.textStartsWith("Download ")), 10_000)?.click() ?: return
        modelsInstalled()
    }

    private fun modelsInstalled() {
        assertTrue("setup finished", device.wait(Until.gone(By.textStartsWith("Set up ")), 45 * 60_000L))
        assertTrue(device.wait(Until.gone(By.textContains("Downloading")), 60_000))
    }

    /** Shares the file into mdoc as a file app would, then adds it in a new case. */
    private fun share(file: File): String {
        // From Home, as the document opens over whatever screen mdoc shows.
        home()
        val send = Intent(Intent.ACTION_SEND).setClassName(MDOC, "$MDOC.MainActivity").setType("application/pdf")
            .putExtra(Intent.EXTRA_STREAM, SharedFiles.uri(file))
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        self.startActivity(send)
        need(By.text("Add document"), 30_000)
        // The sheet's own Add, not the button on Home behind it: the last one in the tree.
        device.wait(Until.findObjects(By.text("Add")), 10_000).last().click()
        val title = file.nameWithoutExtension
        need(By.text(title), 30_000)
        return title
    }

    /**
     * The document's text recognition has started. A PDF with no text layer
     * offers Finish import, a scan Recognize text; either runs OCR.
     */
    private fun recognizing() {
        // mdoc starts reading a new document itself; only one left unread shows the button.
        val first = need(By.text(Pattern.compile("Recognize text|Finish import|$RUNNING")), 60_000)
        if (first.text == "Recognize text" || first.text == "Finish import") {
            first.click()
            need(running, 60_000)
        }
    }

    private fun recognized() {
        // The fixture's own words, read from the page images. Waited for
        // themselves: the progress line comes and goes between stages.
        val text = device.wait(Until.findObject(By.textContains("LOCAL OCR ACCEPTANCE TEST")), 30 * 60_000L)
        assertNotNull("the recognized text shows", text)
        assertTrue(device.findObjects(By.text(Pattern.compile("Resume recognition|Finish import|Recognize text|$RUNNING"))).isEmpty())
    }

    private fun namesFound() {
        // What was found: Replace all, or the note that there was nothing.
        val result = device.wait(Until.findObject(By.text(Pattern.compile("Replace .*|No names found.*"))), 30 * 60_000L)
        assertNotNull("the document shows what was found", result)
        assertTrue(device.findObjects(By.text("Finding names")).isEmpty())
    }

    /**
     * Off screen until the job's foreground service stops: mdoc's process
     * must live throughout. A job that finished leaves no service behind.
     */
    private fun awayWhileTheJobRuns() {
        val deadline = SystemClock.elapsedRealtime() + 30 * 60_000L
        SystemClock.sleep(5_000)
        while (jobRunning()) {
            assertTrue("mdoc kept running off screen", alive())
            assertTrue("the job ended in 30 minutes", SystemClock.elapsedRealtime() < deadline)
            SystemClock.sleep(5_000)
        }
        assertTrue("mdoc is still running after the job", alive())
    }

    /** A job's progress line, as the document shows it while the job runs. */
    private val running = By.text(Pattern.compile(RUNNING))

    private fun need(selector: BySelector, timeout: Long): UiObject2 =
        requireNotNull(device.wait(Until.findObject(selector), timeout)) { "not on screen: $selector" }

    // mdoc's process, from outside

    private fun pid() = shell("pidof $MDOC").trim()

    private fun alive() = pid().isNotEmpty()

    private fun jobRunning() = shell("dumpsys activity services $MDOC").contains("LongJobService")

    /** As Android ends a process for memory: no warning, no chance to save. */
    private fun kill() {
        val pid = pid()
        assertTrue("mdoc is running before the kill", pid.isNotEmpty())
        // The debug build is debuggable, so run-as may signal its process.
        shell("run-as $MDOC kill -9 $pid")
        val deadline = SystemClock.elapsedRealtime() + 10_000
        while (alive() && SystemClock.elapsedRealtime() < deadline) SystemClock.sleep(200)
        assertTrue("mdoc's process was killed", pid() != pid)
        // Android clears the dead task before a new start can land.
        SystemClock.sleep(2_000)
        Log.i(TAG, "killed mdoc ($pid)")
    }

    /** The bytes mdoc keeps, which grow while a download runs. */
    private fun dataKb() = shell("run-as $MDOC du -sk .").trim().substringBefore('\t').substringBefore(' ').toLongOrNull() ?: 0

    private fun shell(command: String): String =
        ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(command)).use { it.readBytes().decodeToString() }

    /** A 120-page scan: the OCR fixture's page as an image, numbered so no two are alike, with no text layer. */
    private fun longScan(): File {
        val source = File(self.cacheDir, "english.pdf")
        if (!source.exists()) self.assets.open("english.pdf").use { input -> source.outputStream().use(input::copyTo) }
        val page = PdfRenderer(ParcelFileDescriptor.open(source, ParcelFileDescriptor.MODE_READ_ONLY)).use { renderer ->
            renderer.openPage(0).use { page ->
                val width = 1000
                Bitmap.createBitmap(width, width * page.height / page.width, Bitmap.Config.ARGB_8888).also {
                    it.eraseColor(Color.WHITE)
                    page.render(it, null, null, PdfRenderer.Page.RENDER_MODE_FOR_PRINT)
                }
            }
        }
        val file = File(self.cacheDir, "Long scan ${System.currentTimeMillis()}.pdf")
        val document = PdfDocument()
        repeat(PAGES) { index ->
            val out = document.startPage(PdfDocument.PageInfo.Builder(595, 842, index + 1).create())
            out.canvas.drawBitmap(page, null, android.graphics.Rect(0, 0, 595, 842), null)
            out.canvas.drawText("Page ${index + 1}", 40f, 820f, android.graphics.Paint().apply { textSize = 14f })
            document.finishPage(out)
        }
        file.outputStream().use(document::writeTo)
        document.close()
        return file
    }

    companion object {
        private const val MDOC = "io.github.lawkitt.mdoc"
        private const val TAG = "MdocLongJobs"
        private const val PAGES = 120
        private const val RUNNING = "Reading the file|Converting|Recognizing text"

        private var failuresBefore = 0
        private var recognizedTitle: String? = null
        private var resumedTitle: String? = null

        /** Crashes and ANRs in mdoc's exit reasons; the test's own kills are SIGNALED. */
        private fun failures(exits: String) = Regex("""reason=\d+ \((ANR|CRASH|CRASH_NATIVE|INITIALIZATION_FAILURE)\)""").findAll(exits).count()

        @BeforeClass
        @JvmStatic
        fun prepare() {
            val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
            fun shell(command: String) = ParcelFileDescriptor.AutoCloseInputStream(automation.executeShellCommand(command)).use { it.readBytes().decodeToString() }
            // Granted as a person would on mdoc's first job, so the prompt does not cover the screen.
            shell("pm grant $MDOC android.permission.POST_NOTIFICATIONS")
            failuresBefore = failures(shell("dumpsys activity exit-info $MDOC"))
        }
    }
}
