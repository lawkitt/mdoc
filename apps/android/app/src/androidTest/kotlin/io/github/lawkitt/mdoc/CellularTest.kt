package io.github.lawkitt.mdoc

import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.setup.Route
import io.github.lawkitt.mdoc.storage.MdocStorage
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * "On cellular, mdoc asks first" (ADR 0088, 0089), on the emulator's own
 * Wi-Fi and cellular: with Wi-Fi off, Download asks; "Wait for Wi-Fi" holds
 * the download and says so; when Wi-Fi returns it finishes, never having
 * used cellular.
 */
@RunWith(AndroidJUnit4::class)
class CellularTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val app get() = instrumentation.targetContext.applicationContext as MdocApplication

    private fun shell(command: String) {
        instrumentation.uiAutomation.executeShellCommand(command).close()
    }

    @After
    fun wifiBack() = shell("svc wifi enable")

    @Test
    fun cellularAsksFirstAndWaitingForWiFiSaysSo() = runBlocking {
        ActivityScenario.launch(MainActivity::class.java).use {
            // Text recognition not installed, so there is something to download.
            app.core.storage.dir(MdocStorage.Area.Models).resolve("ocr").deleteRecursively()
            app.setup.refresh()
            assertTrue(app.setup.state.value.needsSetup)

            shell("svc wifi disable")
            withTimeout(60_000) { app.network.route.first { it == Route.Cellular } }
            instrumentation.runOnMainSync { app.setup.download() }
            assertTrue("asks before cellular", app.setup.state.value.asksCellular)

            instrumentation.runOnMainSync { app.setup.answerCellular(useCellular = false) }
            withTimeout(60_000) { app.setup.state.first { it.waitingForWiFi } }
            assertTrue(app.setup.state.value.working)

            shell("svc wifi enable")
            withTimeout(15 * 60_000) { app.setup.state.first { !it.working } }
            assertEquals(null, app.setup.state.value.error)
            assertTrue(app.setup.state.value.snapshot.ocr?.installed == true)
        }
    }
}
