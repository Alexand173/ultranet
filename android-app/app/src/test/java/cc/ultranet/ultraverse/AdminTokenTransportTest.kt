package cc.ultranet.ultraverse

import cc.ultranet.ultraverse.data.InsecureAdminEndpointException
import cc.ultranet.ultraverse.data.UltraNetOperatorApi
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * The admin bearer token is the node's administrator credential, and it is the one secret this app
 * puts in a request header. It must be refused before the socket opens when the node address is
 * cleartext to a remote host.
 *
 * The refusal is usable as evidence because it happens without any network: if the guard were
 * missing, these calls would instead fail with a connection error after the header had been set.
 */
class AdminTokenTransportTest {

    private fun token() = "a".repeat(32).toCharArray()

    @Test
    fun `the admin token is not sent to a cleartext remote node`() = runBlocking {
        try {
            UltraNetOperatorApi(baseUrl = "http://node.example.org").mineBlock(token())
            fail("a cleartext node must not receive the admin token")
        } catch (error: InsecureAdminEndpointException) {
            assertTrue(error.message!!.contains("https"))
        }
    }

    @Test
    fun `the refusal happens before any socket is opened`() = runBlocking {
        // nodename.invalid does not resolve, so reaching the network at all would surface a different
        // failure; an InsecureAdminEndpointException means nothing was dialled.
        val started = System.currentTimeMillis()
        try {
            UltraNetOperatorApi(baseUrl = "http://node.invalid").mineBlock(token())
            fail("a cleartext node must not receive the admin token")
        } catch (error: InsecureAdminEndpointException) {
            assertTrue(System.currentTimeMillis() - started < 2_000)
        }
    }

    @Test
    fun `a read that carries no token is unaffected by the rule`() = runBlocking {
        // The public ledger read over http is a connection error, not a credential refusal: the rule
        // is about the token, and it must not be smuggled into the read path.
        try {
            UltraNetOperatorApi(baseUrl = "http://node.invalid", connectTimeoutMillis = 500).stats()
            fail("node.invalid must not answer")
        } catch (error: Exception) {
            assertTrue("expected an I/O failure, got ${error::class.java.name}", error !is InsecureAdminEndpointException)
        }
    }
}
