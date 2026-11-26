package io.github.lawkitt.mdoc.settings

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * The user's choices in Settings that the screens follow, in the app's own
 * preferences: the AI connector preview. The twin of iOS's `Preferences`,
 * under the same key.
 */
class Preferences(context: Context) {
    private val prefs = context.getSharedPreferences("preferences", Context.MODE_PRIVATE)
    private val preview = MutableStateFlow(prefs.getBoolean(AI_CONNECTOR_PREVIEW, false))

    /**
     * Settings › AI connector (preview), off until turned on: with it off
     * nothing of Milestone 2 shows (#1164), neither Ask AI nor the
     * "Paste reply?" banner.
     */
    val aiConnectorPreview: StateFlow<Boolean> = preview.asStateFlow()

    fun setAiConnectorPreview(on: Boolean) {
        preview.value = on
        offMain { prefs.edit().putBoolean(AI_CONNECTOR_PREVIEW, on).commit() }
    }

    internal companion object {
        /**
         * Runs a preferences write on one thread of its own, in order, never
         * through `apply()`: Android makes a screen that stops wait for every
         * `apply()` still writing (QueuedWork), and a busy disk turned that
         * into a minute's freeze on the PC's emulator (#1233).
         */
        private val writes = java.util.concurrent.Executors.newSingleThreadExecutor { Thread(it, "mdoc-preferences") }

        internal fun offMain(write: () -> Unit) {
            writes.execute(write)
        }

        /** iOS's `Preferences.Key.aiConnectorPreview`. */
        private const val AI_CONNECTOR_PREVIEW = "aiConnectorPreview"
    }
}
