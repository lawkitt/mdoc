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
        prefs.edit().putBoolean(AI_CONNECTOR_PREVIEW, on).apply()
        preview.value = on
    }

    private companion object {
        /** iOS's `Preferences.Key.aiConnectorPreview`. */
        const val AI_CONNECTOR_PREVIEW = "aiConnectorPreview"
    }
}
