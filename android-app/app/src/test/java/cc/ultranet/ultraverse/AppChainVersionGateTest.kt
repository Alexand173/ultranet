package cc.ultranet.ultraverse

import cc.ultranet.ultraverse.wallet.AppChainSigner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * C8 — the wallet sends the envelope version the node advertises, and refuses to sign one it cannot
 * produce.
 *
 * Before this, `AppChainSigner.TX_VERSION` was a compiled-in literal used in three places: the preimage,
 * the request body and nothing else. It happened to match every node, so the mismatch was invisible — and
 * a version transition would have produced signatures the node rejects, with no way for the wallet to say
 * why. The rules these tests pin are the ones the published contract states (V4, deferred until now).
 */
class AppChainVersionGateTest {
    @Test
    fun theAdvertisedVersionIsUsedWhenThisBuildCanSignIt() {
        assertEquals(5, AppChainSigner.requireSupportedVersion(5))
        assertEquals(setOf(5), AppChainSigner.SUPPORTED_VERSIONS)
    }

    @Test
    fun anUnsupportedAdvertisedVersionRefusesToSign() {
        val failure = runCatching { AppChainSigner.requireSupportedVersion(6) }.exceptionOrNull()

        assertTrue("expected a refusal, got $failure", failure is IllegalArgumentException)
        val message = failure?.message.orEmpty()
        // The message has to name both versions, or the person holding the phone cannot tell whether to
        // update the app or point it at another node.
        assertTrue("the refusal must name the version we sign: $message", message.contains("5"))
        assertTrue("the refusal must name the version the node advertises: $message", message.contains("6"))
    }
}
