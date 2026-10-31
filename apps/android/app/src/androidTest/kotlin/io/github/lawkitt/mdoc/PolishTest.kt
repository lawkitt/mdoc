package io.github.lawkitt.mdoc

import android.content.ClipDescription
import android.content.ClipboardManager
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.document.copy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** The Android polish round: counts read right, restored text is marked sensitive. */
@RunWith(AndroidJUnit4::class)
class PolishTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = instrumentation.targetContext

    @Test
    fun oneIsSingular() {
        val resources = context.resources
        assertEquals("1 mention", resources.getQuantityString(R.plurals.mentions, 1, 1))
        assertEquals("6 mentions", resources.getQuantityString(R.plurals.mentions, 6, 6))
        assertEquals("1 value restored", resources.getQuantityString(R.plurals.values_restored, 1, 1))
        assertEquals("1 page scanned.", resources.getQuantityString(R.plurals.pages_scanned, 1, 1))
    }

    @Test
    fun aRestoredReplyIsSensitiveAndCopyForAiIsNot() = ActivityScenario.launch(MainActivity::class.java).use {
        // On screen: Android shows the clipboard only to the app in front.
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        instrumentation.runOnMainSync {
            copy(context, "Thank you, Maya Bennett.", sensitive = true)
            assertTrue(clipboard.primaryClipDescription!!.extras!!.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE))
            copy(context, "Thank you, PERSON_1.")
            assertFalse(clipboard.primaryClipDescription!!.extras?.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE) ?: false)
        }
    }
}
