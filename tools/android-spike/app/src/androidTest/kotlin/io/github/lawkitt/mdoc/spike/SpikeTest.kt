package io.github.lawkitt.mdoc.spike

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The spike as an instrumentation test, for `adb shell am instrument` and
 * Firebase Test Lab: it runs in the app's process and leaves the report in
 * the app's external files folder. It fails only when the core could not
 * run; budget passes and fails are data, read from the report.
 */
@RunWith(AndroidJUnit4::class)
class SpikeTest {
    @Test
    fun measure() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val report = JSONObject(Harness.measure(context))
        assertFalse(report.optString("error"), report.has("error"))
    }
}
