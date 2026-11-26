package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.wallet.GamesAuthSigner
import cc.ultranet.ultraverse.wallet.WalletCrypto
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith

/** The sign-in digest is protocol: `src/games/tests.rs` pins the same vector for the server. */
@RunWith(AndroidJUnit4::class)
class GamesAuthSignerTest {
    private val address = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e"
    private val id = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"

    private fun hex(bytes: ByteArray) = WalletCrypto.toHex(bytes)

    @Test
    fun theDigestMatchesTheVectorTheServerPins() {
        val digest = GamesAuthSigner.digest("play", address, id, 1_800_000_000, 1_800_000_300)
        assertEquals("2d1074fd8db57d0497d6bf5f07a8963db1806425b147fe5a350233271e60e020", hex(digest))
    }

    @Test
    fun everyFieldIsInsideTheDigest() {
        val base = hex(GamesAuthSigner.digest("play", address, id, 1_800_000_000, 1_800_000_300))
        val otherAddress = "8d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e"
        val otherId = "ff" + id.substring(2)
        assertNotEquals(base, hex(GamesAuthSigner.digest("play", otherAddress, id, 1_800_000_000, 1_800_000_300)))
        assertNotEquals(base, hex(GamesAuthSigner.digest("play", address, otherId, 1_800_000_000, 1_800_000_300)))
        assertNotEquals(base, hex(GamesAuthSigner.digest("play", address, id, 1_800_000_001, 1_800_000_300)))
        assertNotEquals(base, hex(GamesAuthSigner.digest("play", address, id, 1_800_000_000, 1_800_000_301)))
    }

    @Test
    fun aDigestForAnotherPurposeOrABadFieldIsNeverBuilt() {
        assertThrows(IllegalArgumentException::class.java) { GamesAuthSigner.digest("withdraw", address, id, 1, 2) }
        assertThrows(IllegalArgumentException::class.java) { GamesAuthSigner.digest("play", address.uppercase(), id, 1, 2) }
        assertThrows(IllegalArgumentException::class.java) { GamesAuthSigner.digest("play", address, id.substring(2), 1, 2) }
        assertThrows(IllegalArgumentException::class.java) { GamesAuthSigner.digest("play", address, id, 5, 2) }
    }
}
