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
 * Keeps a long job running while mdoc is in the background: a
 * `dataSync` foreground service (Android lists import and local file
 * processing under it) with a notification that names the job and offers
 * Cancel. Started only by [Jobs.run], from a visible screen; Android 15's
 * time limit for the type ends the job as interrupted.
 */
class LongJobService : Service() {
    /** The job this instance last went into the foreground for; 0: none yet. */
    private var job = 0L

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_CANCEL) {
            (application as MdocApplication).jobs.cancel(CancelReason.User)
            return START_NOT_STICKY
        }
        val kind = intent?.getStringExtra(EXTRA_KIND)?.let { runCatching { JobKind.valueOf(it) }.getOrNull() }
        val title = getString(kind?.title ?: R.string.app_name)
        // Always, once started with startForegroundService: Android ends an app
        // whose service does not, even when the job finished first.
        startForeground(NOTIFICATION, notification(title), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        job = intent?.getLongExtra(EXTRA_JOB, 0L) ?: 0L
        started = job
        // Its job ended already, or another took its place: this start
        // stops, never a newer one (#689).
        if (wanted != job) {
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf(startId)
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        // Only its own job's mark: a newer service may be in the foreground already.
        if (started == job) started = 0L
        super.onDestroy()
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
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(title)
            // iOS's subtitle for the same job (BackgroundContinuation).
            .setContentText(getString(R.string.keeps_working))
            .setProgress(0, 0, true)
            .setOngoing(true)
            .setContentIntent(open)
            .addAction(Notification.Action.Builder(null, getString(R.string.cancel), cancel).build())
            .build()
    }

    companion object {
        /** The running job that wants the service in the foreground, by its number; 0: none. */
        @Volatile internal var wanted = 0L

        /**
         * The job whose service has called startForeground and not yet been
         * destroyed; 0: none. Per job, so one job's service, still being
         * destroyed, never makes the next job stop its own before it is in
         * the foreground, which Android ends the app for (#689).
         */
        @Volatile internal var started = 0L

        private val jobs = java.util.concurrent.atomic.AtomicLong()

        /** A new job's number, never 0. */
        internal fun nextJob(): Long = jobs.incrementAndGet()

        private const val CHANNEL = "jobs"
        private const val NOTIFICATION = 1
        private const val EXTRA_KIND = "kind"
        private const val EXTRA_JOB = "job"
        private const val ACTION_CANCEL = "io.github.lawkitt.mdoc.CANCEL_JOB"

        fun intent(context: Context) = Intent(context, LongJobService::class.java)

        fun start(context: Context, kind: JobKind, job: Long) = intent(context).putExtra(EXTRA_KIND, kind.name).putExtra(EXTRA_JOB, job)
    }
}
