package io.github.lawkitt.mdoc.document

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.service.chooser.ChooserResult
import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import io.github.lawkitt.mdoc.core.HandOff
import io.github.lawkitt.mdoc.restore.RestoreActivity

/**
 * What Share as text and Ask AI put in their `ACTION_SEND`: the Vault's
 * text for AI, and the file's pseudonymized name without `.md` as its
 * title, never the document's own title. Nothing else: no notes, no file.
 */
data class SharedText(val text: String, val title: String?) {
    /** The send intent's extras, by key. */
    val extras: Map<String, String>
        get() = buildMap {
            put(Intent.EXTRA_TEXT, text)
            title?.let { put(Intent.EXTRA_TITLE, it) }
        }

    companion object {
        fun of(text: String, fileName: String?) = SharedText(text, fileName?.removeSuffix(".md"))

        /** Ask AI's: Copy for AI's hand-off, as Share as text sends it. */
        fun of(handOff: HandOff) = of(handOff.text, handOff.fileName)
    }
}

/** Where Ask AI's wait for the AI app's answer stands (#1164 decision 11). */
enum class ReplyOffer {
    /** The chooser is up, or was closed without a choice. */
    Sharing,

    /** An app took the text; mdoc has not been in front since. */
    AwaitingReturn,

    /** mdoc is back from the AI app: the "Paste reply?" banner shows. */
    Offered,
}

/**
 * Ask AI's wait for the answer, iOS's `+AskAI` intents: the chooser's
 * choice of an app (its chosen-component callback) stands for iOS's
 * completed activity. Back in front after it, "Paste reply?" shows; a
 * choice reported once mdoc is already back shows it at once. A chooser
 * closed without a choice, or Share's own chooser, offers nothing.
 */
object AskAiWait {
    /** Ask AI's chooser opens. */
    fun asked(): ReplyOffer = ReplyOffer.Sharing

    /** The chooser reports an app took the text; `inFront`: mdoc is resumed. */
    fun chosen(offer: ReplyOffer?, inFront: Boolean): ReplyOffer? = when {
        offer != ReplyOffer.Sharing -> offer
        inFront -> ReplyOffer.Offered
        else -> ReplyOffer.AwaitingReturn
    }

    /** mdoc is in front again. */
    fun resumed(offer: ReplyOffer?): ReplyOffer? = if (offer == ReplyOffer.AwaitingReturn) ReplyOffer.Offered else offer

    /**
     * Whether the chooser's report is an app taking the text: its
     * `ChooserResult` type from Android 15, which also reports its own Copy
     * and Edit (no app the text went to); before, a report is only ever an
     * app picked (null).
     */
    fun pickedAnApp(resultType: Int?): Boolean = resultType == null || resultType == ChooserResult.CHOOSER_RESULT_SELECTED_COMPONENT
}

/** The chooser's callback when an app is picked: only mdoc's own broadcast. */
private const val CHOSEN = "io.github.lawkitt.mdoc.ASK_AI_CHOSEN"

/**
 * Ask AI's chooser: Share as text's intent (the same [SharedText]), which
 * reports the app picked, without mdoc's own Restore with mdoc, no AI app.
 */
fun askAiIntent(handOff: HandOff, context: Context): Intent {
    val chosen = PendingIntent.getBroadcast(
        context,
        0,
        Intent(CHOSEN).setPackage(context.packageName),
        // Mutable: the chooser adds the component it started.
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
    )
    return chooser(SharedText.of(handOff), chosen.intentSender)
        .putExtra(Intent.EXTRA_EXCLUDE_COMPONENTS, arrayOf(ComponentName(context, RestoreActivity::class.java)))
}

/** Calls `onChosen` each time Ask AI's chooser starts an app (not its own Copy or Edit), until unregistered. */
fun registerAskAiChosen(context: Context, onChosen: () -> Unit): BroadcastReceiver {
    val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.VANILLA_ICE_CREAM) {
                IntentCompat.getParcelableExtra(intent, Intent.EXTRA_CHOOSER_RESULT, ChooserResult::class.java)?.type
            } else {
                null
            }
            if (AskAiWait.pickedAnApp(type)) onChosen()
        }
    }
    ContextCompat.registerReceiver(context, receiver, IntentFilter(CHOSEN), ContextCompat.RECEIVER_NOT_EXPORTED)
    return receiver
}

/** The share sheet for `shared`, titled by its title; `chosen` hears the app picked. */
internal fun chooser(shared: SharedText, chosen: android.content.IntentSender? = null): Intent {
    val send = Intent(Intent.ACTION_SEND).setType("text/plain")
    shared.extras.forEach { (key, value) -> send.putExtra(key, value) }
    return Intent.createChooser(send, shared.title, chosen)
}
