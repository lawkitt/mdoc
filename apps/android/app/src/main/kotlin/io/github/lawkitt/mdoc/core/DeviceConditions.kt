package io.github.lawkitt.mdoc.core

import android.app.ActivityManager
import android.content.Context
import android.os.PowerManager
import uniffi.mdoc_ffi.Thermal

/** What a scan asks about the phone (ADR 0120): the twin of the iOS `DeviceConditions`. */
interface DeviceConditions {
    fun thermal(): Thermal

    /** Bytes free to allocate now, or null where the phone cannot tell. */
    fun freeMemory(): Long?

    companion object {
        /** The phone itself: `PowerManager`'s thermal status, `ActivityManager.MemoryInfo`. */
        fun of(context: Context): DeviceConditions = object : DeviceConditions {
            private val power = context.getSystemService(PowerManager::class.java)
            private val activities = context.getSystemService(ActivityManager::class.java)

            override fun thermal() = fromStatus(power.currentThermalStatus)

            override fun freeMemory(): Long = ActivityManager.MemoryInfo().also(activities::getMemoryInfo).availMem
        }

        /** Nothing known: no pause and no free-memory check. */
        val Unknown: DeviceConditions = object : DeviceConditions {
            override fun thermal() = Thermal.NOMINAL

            override fun freeMemory(): Long? = null
        }

        /** Android's statuses on iOS's four steps, as the core counts them. */
        fun fromStatus(status: Int): Thermal = when {
            status >= PowerManager.THERMAL_STATUS_CRITICAL -> Thermal.CRITICAL
            status >= PowerManager.THERMAL_STATUS_MODERATE -> Thermal.SERIOUS
            status == PowerManager.THERMAL_STATUS_LIGHT -> Thermal.FAIR
            else -> Thermal.NOMINAL
        }
    }
}
