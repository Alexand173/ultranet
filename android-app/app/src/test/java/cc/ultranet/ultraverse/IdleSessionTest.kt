package cc.ultranet.ultraverse

import cc.ultranet.ultraverse.wallet.IdleSession
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The unlock timer, tested with an injected clock: a policy that can only be checked by waiting
 * fifteen minutes is a policy nobody checks.
 */
class IdleSessionTest {

    private val timeout = IdleSession.DEFAULT_TIMEOUT_MILLIS
    private val start = 1_000_000L

    @Test
    fun `a session with no recorded activity does not lock on the first poll`() {
        val session = IdleSession(timeout)

        assertNull(session.elapsedMillis(start))
        assertFalse(session.shouldLock(start))
    }

    @Test
    fun `a session inside the timeout stays unlocked`() {
        val session = IdleSession(timeout)
        session.record(start)

        assertFalse(session.shouldLock(start + timeout - 1))
    }

    @Test
    fun `a session that reaches the timeout locks`() {
        val session = IdleSession(timeout)
        session.record(start)

        assertTrue(session.shouldLock(start + timeout))
        assertTrue(session.shouldLock(start + timeout * 3))
    }

    @Test
    fun `activity resets the clock`() {
        val session = IdleSession(timeout)
        session.record(start)
        session.record(start + timeout - 1)

        assertFalse(session.shouldLock(start + timeout))
        assertTrue(session.shouldLock(start + timeout - 1 + timeout))
    }

    @Test
    fun `clearing the session stops it from ever reporting idle`() {
        val session = IdleSession(timeout)
        session.record(start)
        session.clear()

        assertNull(session.elapsedMillis(start + timeout * 10))
        assertFalse(session.shouldLock(start + timeout * 10))
    }

    @Test
    fun `a clock that moves backwards reports zero elapsed rather than locking`() {
        val session = IdleSession(timeout)
        session.record(start)

        assertFalse(session.shouldLock(start - 60_000))
    }
}
