package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.wallet.GameAdminSigner
import cc.ultranet.ultraverse.wallet.WalletCrypto
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith

/** The owner digest is protocol: `src/games/tests.rs` pins the same vectors for the server. */
@RunWith(AndroidJUnit4::class)
class GameAdminSignerTest {
    private val nonce = "ab".repeat(16)
    private val game = "00112233445566778899aabbccddeeff"

    private fun hex(bytes: ByteArray) = WalletCrypto.toHex(bytes)

    @Test
    fun theWithdrawAndRegisterDigestsMatchTheServerVectors() {
        assertEquals(
            "8006596d5a34eaf610a746f8ef3bf172dddd2c374294ef27562a68ea85d62930",
            hex(GameAdminSigner.digest("pool-withdraw", 7, game, "400", "", "", nonce, 1_800_000_000)),
        )
        assertEquals(
            "f78faa7dee8619104d2ed305802a17a7ba5aaffdb87df06c257ed1ed9abd29cd",
            hex(GameAdminSigner.digest("register", 7, "", "Echo Game", "a short description", "", nonce, 1_800_000_000)),
        )
    }

    @Test
    fun everyFieldIsInsideTheDigestSoASignatureCannotBeMoved() {
        val base = hex(GameAdminSigner.digest("pool-withdraw", 7, game, "400", "", "", nonce, 1_800_000_000))
        assertNotEquals(base, hex(GameAdminSigner.digest("pool-fund", 7, game, "400", "", "", nonce, 1_800_000_000)))
        assertNotEquals(base, hex(GameAdminSigner.digest("pool-withdraw", 8, game, "400", "", "", nonce, 1_800_000_000)))
        assertNotEquals(base, hex(GameAdminSigner.digest("pool-withdraw", 7, game.replace('0', '1'), "400", "", "", nonce, 1_800_000_000)))
        assertNotEquals(base, hex(GameAdminSigner.digest("pool-withdraw", 7, game, "401", "", "", nonce, 1_800_000_000)))
        assertNotEquals(base, hex(GameAdminSigner.digest("pool-withdraw", 7, game, "400", "", "", "cd".repeat(16), 1_800_000_000)))
        assertNotEquals(base, hex(GameAdminSigner.digest("pool-withdraw", 7, game, "400", "", "", nonce, 1_800_000_001)))
    }

    @Test
    fun aBadChainOrNonceIsNeverSigned() {
        assertThrows(IllegalArgumentException::class.java) { GameAdminSigner.digest("register", 0, "", "x", "", "", nonce, 1) }
        assertThrows(IllegalArgumentException::class.java) { GameAdminSigner.digest("register", 7, "", "x", "", "", "short", 1) }
        assertThrows(IllegalArgumentException::class.java) { GameAdminSigner.digest("register", 7, "", "x", "", "", nonce.uppercase(), 1) }
    }

    @Test
    fun thePolicySummaryIsTheTextTheServiceRebuilds() {
        assertEquals("100|win|3|100|0", GameAdminSigner.policySummary(100, "win", 3, 100, 0))
    }
}
