package io.github.lawkitt.mdoc.document

import io.github.lawkitt.mdoc.core.HandOffRefusal
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * The line under ⋯'s Find names step while a job runs (#1295): the
 * document's own scan is not "another document".
 */
class StepWaitTest {
    @Test
    fun theDocumentsOwnJobIsNotAnotherDocument() {
        assertEquals(StepWait.OwnJob, StepWait.of(HandOffRefusal.Step.FindNames, jobRunning = true, jobIsMine = true))
    }

    @Test
    fun anotherDocumentsJobMakesItWait() {
        assertEquals(StepWait.OtherJob, StepWait.of(HandOffRefusal.Step.FindNames, jobRunning = true, jobIsMine = false))
    }

    @Test
    fun nothingWaitsWithoutAJob() {
        assertNull(StepWait.of(HandOffRefusal.Step.FindNames, jobRunning = false, jobIsMine = false))
    }

    @Test
    fun onlyFindNamesWaits() {
        assertNull(StepWait.of(HandOffRefusal.Step.Review, jobRunning = true, jobIsMine = false))
        assertNull(StepWait.of(HandOffRefusal.Step.ReplaceAndCopy(2), jobRunning = true, jobIsMine = true))
        assertNull(StepWait.of(null, jobRunning = true, jobIsMine = true))
    }
}
