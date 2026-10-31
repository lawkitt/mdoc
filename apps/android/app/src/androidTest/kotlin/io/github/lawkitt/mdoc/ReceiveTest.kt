package io.github.lawkitt.mdoc

import android.content.Intent
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.core.content.FileProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Open in mdoc: a PDF shared from another app asks for its title and case. */
@RunWith(AndroidJUnit4::class)
class ReceiveTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createEmptyComposeRule()

    @Test
    fun aSharedPdfOpensTheAddSheet() {
        val app = core.app
        // Another app's file, as a content URI (here from the share folder the provider exposes).
        val folder = File(app.cacheDir, "hand-off").apply { mkdirs() }
        val pdf = File(folder, "english.pdf")
        InstrumentationRegistry.getInstrumentation().context.assets.open("english.pdf").use { input -> pdf.outputStream().use(input::copyTo) }
        val uri = FileProvider.getUriForFile(app, "${app.packageName}.files", pdf)
        val share = Intent(app, MainActivity::class.java).setAction(Intent.ACTION_SEND).setType("application/pdf")
            .putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        ActivityScenario.launch<MainActivity>(share).use {
            compose.waitUntil(15_000) { compose.onAllNodes(hasText("Add document")).fetchSemanticsNodes().isNotEmpty() }
        }
    }
}
