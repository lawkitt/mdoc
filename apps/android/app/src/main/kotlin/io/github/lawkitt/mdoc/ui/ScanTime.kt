package io.github.lawkitt.mdoc.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import io.github.lawkitt.mdoc.R
import kotlin.math.roundToInt
import uniffi.mdoc_ffi.FloorReason

/** How long a scan takes, in words: the twin of the iOS `ScanTime`. */
object ScanTime {
    /** While a scan runs: "About 3 min left", or under a minute. */
    @Composable
    fun left(seconds: Long): String =
        if (seconds < 60) stringResource(R.string.less_than_a_minute_left) else stringResource(R.string.minutes_left, minutes(seconds))

    /** Settings: "About 2 min per 10 pages", or seconds under 90. */
    @Composable
    fun perTenPages(seconds: Long): String =
        if (seconds < 90) {
            stringResource(R.string.seconds_per_ten_pages, seconds.toInt())
        } else {
            stringResource(R.string.minutes_per_ten_pages, minutes(seconds))
        }

    /** Below the floor, where Find names would be and in Settings: why. */
    @Composable
    fun belowFloor(reason: FloorReason?): String =
        stringResource(if (reason == FloorReason.SPEED) R.string.below_floor_slow_note else R.string.below_floor_note)

    private fun minutes(seconds: Long) = (seconds / 60.0).roundToInt()
}
