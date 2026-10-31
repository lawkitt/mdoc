package io.github.lawkitt.mdoc.spike

import android.app.Activity
import android.content.Intent
import android.graphics.Typeface
import android.os.Bundle
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import kotlin.concurrent.thread

/**
 * Runs the spike on launch and shows the report, with Share so a phone that
 * is not on the PC (a friend's) can send it back. Keep the screen on and the
 * phone unplugged and cool while it runs; it takes a few minutes.
 */
class SpikeActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val status = TextView(this).apply {
            typeface = Typeface.MONOSPACE
            textSize = 11f
            setTextIsSelectable(true)
            text = "Running the feasibility spike…"
        }
        val share = Button(this).apply {
            text = "Share report"
            isEnabled = false
        }
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(32, 32, 32, 32)
                addView(share)
                addView(ScrollView(context).apply { addView(status) })
            },
        )
        thread(name = "spike") {
            val report = Harness.measure(applicationContext)
            runOnUiThread {
                status.text = report
                share.isEnabled = true
                share.setOnClickListener {
                    val send = Intent(Intent.ACTION_SEND)
                        .setType("text/plain")
                        .putExtra(Intent.EXTRA_SUBJECT, "mdoc spike report")
                        .putExtra(Intent.EXTRA_TEXT, report)
                    startActivity(Intent.createChooser(send, "Share report"))
                }
            }
        }
    }
}
