package io.github.lawkitt.mdoc.core

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** A Vault original being viewed: decrypted once, and wiped on close (#1091 Decision 24). */
class OriginalDataTest {
    @Test
    fun anOriginalIsDecryptedOnceAndDroppedOnClose() {
        val decrypts = AtomicInteger()
        val removes = AtomicInteger()
        val bytes = byteArrayOf(1, 2, 3)
        val stored = OriginalData.Stored(
            decrypt = {
                decrypts.incrementAndGet()
                OriginalData.Decrypted.Memory(bytes.copyOf())
            },
            remove = { removes.incrementAndGet() },
        )
        val first = stored.open() as OriginalData.Decrypted.Memory
        assertSame(first, stored.open())
        assertEquals(1, decrypts.get())

        stored.close()
        assertArrayEquals("its bytes are wiped", byteArrayOf(0, 0, 0), first.bytes)
        assertEquals(1, removes.get())
        // Shown again, it is decrypted again.
        assertArrayEquals(bytes, (stored.open() as OriginalData.Decrypted.Memory).bytes)
        assertEquals(2, decrypts.get())
    }

    /** A close while the original decrypts neither waits for it nor lets it keep anything. */
    @Test
    fun aCloseDuringADecryptKeepsNothing() {
        val started = CountDownLatch(1)
        val release = CountDownLatch(1)
        val removes = AtomicInteger()
        var kept: ByteArray? = null
        val stored = OriginalData.Stored(
            decrypt = {
                started.countDown()
                release.await(10, TimeUnit.SECONDS)
                OriginalData.Decrypted.Memory(byteArrayOf(7, 7)).also { kept = it.bytes }
            },
            remove = { removes.incrementAndGet() },
        )
        var failure: Throwable? = null
        val viewer = thread { failure = runCatching { stored.open() }.exceptionOrNull() }
        assertTrue(started.await(10, TimeUnit.SECONDS))

        stored.close()
        release.countDown()
        viewer.join(10_000)

        assertTrue("the view gets nothing", failure is IllegalStateException)
        assertArrayEquals("what it decrypted is wiped", byteArrayOf(0, 0), kept)
        assertEquals("the close and the overtaken decrypt each remove the copy", 2, removes.get())
    }
}
