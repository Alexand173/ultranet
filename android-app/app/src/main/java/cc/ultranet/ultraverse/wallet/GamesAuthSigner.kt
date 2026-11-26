package cc.ultranet.ultraverse.wallet

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.charset.StandardCharsets

/** What the games service needs to verify a sign-in: the wallet's public key and its signature, both hex. */
internal class SignedChallenge(val publicKeyHex: String, val signatureHex: String)

/**
 * Signs the games service's one-time sign-in challenge, mirroring `challenge_digest` in `src/games/auth.rs`:
 *
 * ```
 * digest = SHA3-256("UltraNet/wallet-challenge/v1"
 *                   ‖ u32le(len) ‖ purpose ‖ u32le(len) ‖ address
 *                   ‖ challenge id (32 bytes) ‖ u64le(issued_at) ‖ u64le(expires_at))
 * ```
 *
 * The wallet recomputes the digest from the challenge's own fields and never signs a digest the server
 * hands it, so the domain string, the address and the times are always the ones the wallet checked. The
 * domain is its own: a signature made here can never be mistaken for a transaction.
 * `GamesAuthSignerTest` pins a vector that `src/games/tests.rs` pins as well.
 */
internal object GamesAuthSigner {
    const val PURPOSE_PLAY = "play"
    private val DOMAIN = "UltraNet/wallet-challenge/v1".toByteArray(StandardCharsets.UTF_8)
    private val ADDRESS_PATTERN = Regex("^[0-9a-f]{64}$")
    private val ID_PATTERN = Regex("^[0-9a-f]{64}$")

    fun digest(purpose: String, address: String, challengeIdHex: String, issuedAt: Long, expiresAt: Long): ByteArray {
        require(purpose == PURPOSE_PLAY) { "This wallet signs the games sign-in for the purpose \"play\" only." }
        require(ADDRESS_PATTERN.matches(address)) { "The address must be 64 lowercase hexadecimal characters." }
        require(ID_PATTERN.matches(challengeIdHex)) { "The challenge id must be 64 lowercase hexadecimal characters." }
        require(issuedAt in 0..expiresAt) { "The challenge times are not valid." }
        val preimage = ByteArrayOutputStream(160)
        preimage.write(DOMAIN)
        for (part in listOf(purpose, address)) {
            val bytes = part.toByteArray(StandardCharsets.UTF_8)
            preimage.write(leInt(bytes.size))
            preimage.write(bytes)
        }
        preimage.write(ByteArray(32) { challengeIdHex.substring(it * 2, it * 2 + 2).toInt(16).toByte() })
        preimage.write(leLong(issuedAt))
        preimage.write(leLong(expiresAt))
        val bytes = preimage.toByteArray()
        return try {
            NativeDilithium.sha3_256(bytes)
        } finally {
            bytes.fill(0)
        }
    }

    /** Signs [digest] with the wallet key. It refuses a seed that does not derive [address]. */
    fun sign(seed: ByteArray, address: String, digest: ByteArray): SignedChallenge {
        require(seed.size == WALLET_SEED_BYTES) { "The local wallet seed has an unexpected size." }
        require(digest.size == 32) { "A sign-in digest is 32 bytes." }
        val publicKey = NativeDilithium.publicKey(seed)
        try {
            require(publicKey.size == DILITHIUM_PUBLIC_KEY_BYTES) { "The local wallet key could not be loaded." }
            val addressDigest = NativeDilithium.sha3_256(publicKey)
            val derived = try {
                WalletCrypto.toHex(addressDigest)
            } finally {
                addressDigest.fill(0)
            }
            require(derived == address) { "The wallet identity does not match its signing key." }
            val signature = NativeDilithium.sign(seed, digest)
            require(signature.size == DILITHIUM_SIGNATURE_BYTES) { "Dilithium-5 returned an incompatible signature size." }
            return SignedChallenge(WalletCrypto.toHex(publicKey), WalletCrypto.toHex(signature))
        } finally {
            publicKey.fill(0)
        }
    }

    private fun leLong(value: Long): ByteArray = ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putLong(value).array()

    private fun leInt(value: Int): ByteArray = ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN).putInt(value).array()
}
