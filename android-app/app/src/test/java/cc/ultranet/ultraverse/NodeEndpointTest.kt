package cc.ultranet.ultraverse

import cc.ultranet.ultraverse.data.NodeEndpoint
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The node address is what every request is concatenated onto, so these rules are the difference
 * between "talk to my node" and "talk to a host of somebody else's choosing".
 */
class NodeEndpointTest {

    @Test
    fun `the default node is accepted unchanged`() {
        val verdict = NodeEndpoint.check(NodeEndpoint.DEFAULT)

        assertTrue(verdict is NodeEndpoint.Verdict.Accepted)
        assertEquals(NodeEndpoint.DEFAULT, (verdict as NodeEndpoint.Verdict.Accepted).baseUrl)
    }

    @Test
    fun `surrounding whitespace and a trailing slash are normalised away`() {
        val verdict = NodeEndpoint.check("  https://node.example.org/  ")

        assertEquals(
            "https://node.example.org",
            (verdict as NodeEndpoint.Verdict.Accepted).baseUrl,
        )
    }

    @Test
    fun `a port survives normalisation`() {
        val verdict = NodeEndpoint.check("https://node.example.org:8443")

        assertEquals(
            "https://node.example.org:8443",
            (verdict as NodeEndpoint.Verdict.Accepted).baseUrl,
        )
    }

    @Test
    fun `cleartext to a remote host is refused`() {
        val verdict = NodeEndpoint.check("http://node.example.org")

        assertTrue(verdict is NodeEndpoint.Verdict.Rejected)
        assertTrue((verdict as NodeEndpoint.Verdict.Rejected).reason.contains("https"))
    }

    @Test
    fun `cleartext to loopback is allowed, because that is a node on this device`() {
        val verdict = NodeEndpoint.check("http://127.0.0.1:8081")

        assertEquals(
            "http://127.0.0.1:8081",
            (verdict as NodeEndpoint.Verdict.Accepted).baseUrl,
        )
    }

    @Test
    fun `a base address with a path is refused, because paths are appended by the client`() {
        val verdict = NodeEndpoint.check("https://node.example.org/api")

        assertTrue(verdict is NodeEndpoint.Verdict.Rejected)
        assertTrue((verdict as NodeEndpoint.Verdict.Rejected).reason.contains("path"))
    }

    @Test
    fun `credentials in the address are refused`() {
        val verdict = NodeEndpoint.check("https://user:secret@node.example.org")

        assertTrue(verdict is NodeEndpoint.Verdict.Rejected)
    }

    @Test
    fun `a blank address is refused rather than silently defaulted`() {
        val verdict = NodeEndpoint.check("   ")

        assertTrue(verdict is NodeEndpoint.Verdict.Rejected)
    }

    @Test
    fun `the label shows the host so two nodes cannot look alike`() {
        assertEquals("node.example.org", NodeEndpoint.label("https://node.example.org"))
        assertEquals("127.0.0.1:8081", NodeEndpoint.label("http://127.0.0.1:8081"))
    }
}
