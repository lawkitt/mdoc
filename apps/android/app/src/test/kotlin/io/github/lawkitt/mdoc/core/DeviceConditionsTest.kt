package io.github.lawkitt.mdoc.core

import android.os.PowerManager
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.mdoc_ffi.Thermal

/** Android's thermal statuses on the core's four steps (ADR 0120): the scan pauses only at critical. */
class DeviceConditionsTest {
    @Test
    fun statusesMapOntoTheCoresSteps() {
        val steps = mapOf(
            PowerManager.THERMAL_STATUS_NONE to Thermal.NOMINAL,
            PowerManager.THERMAL_STATUS_LIGHT to Thermal.FAIR,
            PowerManager.THERMAL_STATUS_MODERATE to Thermal.SERIOUS,
            PowerManager.THERMAL_STATUS_SEVERE to Thermal.SERIOUS,
            PowerManager.THERMAL_STATUS_CRITICAL to Thermal.CRITICAL,
            PowerManager.THERMAL_STATUS_EMERGENCY to Thermal.CRITICAL,
            PowerManager.THERMAL_STATUS_SHUTDOWN to Thermal.CRITICAL,
        )
        for ((status, step) in steps) assertEquals("status $status", step, DeviceConditions.fromStatus(status))
    }
}
