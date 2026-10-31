package io.github.lawkitt.mdoc

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.test.DeviceConfigurationOverride
import androidx.compose.ui.test.FontScale
import androidx.compose.ui.test.ForcedSize
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.then
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.DeviceTier
import io.github.lawkitt.mdoc.core.ModelOffer
import io.github.lawkitt.mdoc.core.SetupSnapshot
import io.github.lawkitt.mdoc.setup.SetupCard
import io.github.lawkitt.mdoc.setup.SetupState
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.ModelId

/** Regressions from the review of model setup (lawkitt/mdoc#316). */
@RunWith(AndroidJUnit4::class)
class SetupFixesTest {
    @get:Rule
    val core = TestCore()

    @get:Rule
    val compose = createComposeRule()

    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val app get() = core.app

    @Test
    fun aCoreThatFailsToOpenShowsOnHomeAndDoesNotCrash() {
        app.openCore = { error("The stored case key is damaged.") }
        app.reopenCore()
        ActivityScenario.launch(MainActivity::class.java).use {
            compose.waitUntil(10_000) {
                compose.onAllNodes(hasText("mdoc could not open its documents")).fetchSemanticsNodes().isNotEmpty()
            }
        }
    }

    @Test
    fun aRefusedDownloadIsFetchedAfreshOnRetry() = runBlocking {
        ActivityScenario.launch(MainActivity::class.java).use {
            val models = app.core.models
            val file = models.remainingFiles(ModelId.OCR_ENGLISH_RUSSIAN).first()
            // A full-size file of the wrong bytes where the download resumes.
            val part = File(app.core.storage.dir(MdocStorage.Area.Downloads), "${ModelId.OCR_ENGLISH_RUSSIAN.name}/${file.path}.part")
            part.parentFile!!.mkdirs()
            part.writeBytes(ByteArray(file.bytes.toInt()))
            app.setup.refresh()
            instrumentation.runOnMainSync { app.setup.download() }
            withTimeout(15 * 60_000) { app.setup.state.first { it.working } }
            withTimeout(15 * 60_000) { app.setup.state.first { !it.working } }
            // Refused once, and thrown away.
            assertTrue(app.setup.state.value.error != null)
            assertTrue(!part.exists())
            instrumentation.runOnMainSync { app.setup.download() }
            withTimeout(15 * 60_000) { app.setup.state.first { it.working } }
            withTimeout(15 * 60_000) { app.setup.state.first { !it.working } }
            assertNull(app.setup.state.value.error)
            assertEquals(true, app.setup.state.value.snapshot.ocr?.installed)
        }
    }

    @Test
    fun aResumedSetupFetchesOnlyTheFilesLeft() = runBlocking {
        val models = app.core.models
        val files = models.remainingFiles(ModelId.OCR_ENGLISH_RUSSIAN)
        assertTrue(files.size > 1)
        // One file downloaded and installed, then setup stopped.
        val first = files.first()
        val downloaded = File(app.cacheDir, "first.part")
        io.github.lawkitt.mdoc.setup.ModelDownloader({ app.network.wifi }, { false }, {}).download(first.url, downloaded) {}
        models.install(ModelId.OCR_ENGLISH_RUSSIAN, first, downloaded)
        val left = models.remainingFiles(ModelId.OCR_ENGLISH_RUSSIAN)
        assertEquals(files.drop(1).map { it.path }, left.map { it.path })
        app.setup.refresh()
        assertEquals(left.sumOf { it.bytes.toLong() }, app.setup.state.value.missingBytes)
    }

    @Test
    fun theCardsButtonsAreReachableAtLargeTextOnAShortScreen() {
        val state = SetupState(
            SetupSnapshot(
                DeviceTier.Detector,
                listOf(
                    ModelOffer(ModelId.GLINER2_FP16, "Name detector", 649_500_000, installed = false),
                    ModelOffer(ModelId.OCR_ENGLISH_RUSSIAN, "Text recognition", 50_000_000, installed = false),
                ),
            ),
        )
        compose.setContent {
            DeviceConfigurationOverride(DeviceConfigurationOverride.FontScale(2f) then DeviceConfigurationOverride.ForcedSize(DpSize(360.dp, 420.dp))) {
                Column(androidx.compose.ui.Modifier.verticalScroll(rememberScrollState())) {
                    SetupCard(state, onDownload = {}, onNotNow = {}, onCancel = {}, onUseCellular = {})
                }
            }
        }
        compose.onNodeWithText("Not now").performScrollTo().assertIsDisplayed()
    }

    @Test
    fun withEverythingInstalledTheCardOffersNoDownload() {
        val state = SetupState(
            SetupSnapshot(DeviceTier.BelowFloor, listOf(ModelOffer(ModelId.OCR_ENGLISH_RUSSIAN, "Text recognition", 18_000_000, installed = true))),
        )
        compose.setContent { SetupCard(state, onDownload = {}, onNotNow = null, onCancel = {}, onUseCellular = {}) }
        compose.onNodeWithText("Installed", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Download", substring = true).assertDoesNotExist()
        compose.onNodeWithText("Set up", substring = true).assertDoesNotExist()
    }
}
