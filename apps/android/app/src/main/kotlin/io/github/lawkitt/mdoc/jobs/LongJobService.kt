package io.github.lawkitt.mdoc.jobs

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import io.github.lawkitt.mdoc.MainActivity
import io.github.lawkitt.mdoc.MdocApplication
import io.github.lawkitt.mdoc.R

/**
 * Keeps a long job running while mdoc is in the background (ADR 0089): a
 * `dataSync` foreground service (Android lists import and local file
 * processing under it) with a notification that names the job and offers
 * Cancel. Started only by [Jobs.run], from a visible screen; Android 15's
 * time limit for the type ends the job as interrupted.
 */
class LongJobService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_CANCEL) {
            (application as MdocApplication).jobs.cancel(CancelReason.User)
            return START_NOT_STICKY
        }
        val title = intent?.getStringExtra(EXTRA_TITLE) ?: getString(R.string.app_name)
        startForeground(NOTIFICATION, notification(title), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        return START_NOT_STICKY
    }

    override fun onTimeout(startId: Int, fgsType: Int) {
        (application as MdocApplication).jobs.cancel(CancelReason.Background)
        stopSelf()
    }

    private fun notification(title: String): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.jobs_channel), NotificationManager.IMPORTANCE_LOW),
        )
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val cancel = PendingIntent.getService(
            this, 1, Intent(this, LongJobService::class.java).setAction(ACTION_CANCEL), PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(title)
            .setProgress(0, 0, true)
            .setOngoing(true)
            .setContentIntent(open)
            .addAction(Notification.Action.Builder(null, getString(R.string.cancel), cancel).build())
            .build()
    }

    companion object {
        private const val CHANNEL = "jobs"
        private const val NOTIFICATION = 1
        private const val EXTRA_TITLE = "title"
        private const val ACTION_CANCEL = "io.github.lawkitt.mdoc.CANCEL_JOB"

        fun intent(context: Context) = Intent(context, LongJobService::class.java)

        fun start(context: Context, title: String) = intent(context).putExtra(EXTRA_TITLE, title)
    }
}
