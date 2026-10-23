package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.wallet.NativeDilithium
import cc.ultranet.ultraverse.wallet.StoredWallet
import cc.ultranet.ultraverse.wallet.TransactionSigner
import cc.ultranet.ultraverse.wallet.WALLET_KDF_ITERATIONS
import cc.ultranet.ultraverse.wallet.WalletCrypto
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.Executors

@RunWith(AndroidJUnit4::class)
class WalletCryptoParityTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"

    @Test
    fun bip39AndUltraNetSeedMatchWebsiteKnownAnswer() {
        assertTrue(WalletCrypto.isValidRecoveryPhrase(context, phrase))
        assertTrue(WalletCrypto.isValidRecoveryPhrase(context, "  ABANDON   abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about  "))
        assertFalse(WalletCrypto.isValidRecoveryPhrase(context, "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon"))
        val seed = WalletCrypto.deriveSeed(context, phrase)
        try {
            assertEquals("86b21c2b9e5d6e4f6d71f324da1db33565ece8c86448a1eb1592a5271aae2847", WalletCrypto.toHex(seed))
        } finally {
            seed.fill(0)
        }
    }

    @Test
    fun seededDilithiumPublicAddressAndSignatureMatchWebsiteWasm() {
        val seed = WalletCrypto.deriveSeed(context, phrase)
        val digest = ByteArray(32) { it.toByte() }
        try {
            val identity = WalletCrypto.deriveIdentity(seed)
            assertEquals("225f3c24df3c9a598d8484def6572a12eebea04a5ba9aa0955062332f916d118", identity.address)
            val signature = NativeDilithium.sign(seed, digest)
            try {
                assertEquals(4_627, signature.size)
                assertTrue(NativeDilithium.verify(WalletCrypto.publicKeyFromHex(identity.publicKeyHex), digest, signature))
                assertEquals("66c597479c3ddd779487af1e701184066f9b887089fa7a4ee564581f72210223", WalletCrypto.toHex(NativeDilithium.sha3_256(signature)))
                val tampered = signature.copyOf().also { it[signature.lastIndex] = (it.last().toInt() xor 1).toByte() }
                try {
                    assertFalse(NativeDilithium.verify(WalletCrypto.publicKeyFromHex(identity.publicKeyHex), digest, tampered))
                } finally {
                    tampered.fill(0)
                }
            } finally {
                signature.fill(0)
            }
        } finally {
            seed.fill(0)
            digest.fill(0)
        }
    }

    @Test
    fun nativeTransferSignerProducesAValidDilithiumVersionOneEnvelope() {
        val seed = WalletCrypto.deriveSeed(context, phrase)
        val recipient = "22".repeat(32)
        try {
            val identity = WalletCrypto.deriveIdentity(seed)
            val signed = TransactionSigner.sign(
                seed = seed,
                publicKeyHex = identity.publicKeyHex,
                sender = identity.address,
                recipient = recipient,
                amount = 1_000_000,
                fee = 10_000,
                nonce = 0,
                timestamp = 1_700_000_000,
            )
            val publicKey = WalletCrypto.publicKeyFromHex(identity.publicKeyHex)
            val digest = TransactionSigner.buildLegacyTransactionDigest(
                sender = signed.sender,
                recipient = signed.recipient,
                amount = signed.amount,
                fee = signed.fee,
                timestamp = signed.timestamp,
                nullifier = signed.nullifier,
                nonce = signed.nonce,
                gasLimit = signed.gasLimit,
                gasPrice = signed.gasPrice,
            )
            try {
                assertEquals(0, signed.chainId)
                assertEquals(1, signed.version)
                assertTrue(NativeDilithium.verify(publicKey, digest, signed.signature))
            } finally {
                publicKey.fill(0)
                digest.fill(0)
                signed.senderPublicKey.fill(0)
                signed.wipeTransient()
            }
        } finally {
            seed.fill(0)
        }
    }

    @Test
    fun versionOneDigestMatchesRustAndWebsiteVector() {
        val sender = "11".repeat(32)
        val recipient = "22".repeat(32)
        val nullifier = ByteArray(32) { it.toByte() }
        val digest = TransactionSigner.buildLegacyTransactionDigest(
            sender = sender,
            recipient = recipient,
            amount = 25_000_000,
            fee = 250_000,
            timestamp = 1_700_000_000,
            nullifier = nullifier,
            nonce = 0,
            gasLimit = 500_000,
            gasPrice = 1,
        )
        try {
            assertEquals("f968acd8ef5f17f72eed6d71d1c4ba9a03de4bbb5c28f3da2718ef6d18079c72", WalletCrypto.toHex(digest))
        } finally {
            digest.fill(0)
            nullifier.fill(0)
        }
    }

    @Test
    fun encryptedSeedAuthenticatesMetadataAndRejectsWrongPassword() {
        val seed = WalletCrypto.deriveSeed(context, phrase)
        val password = "correct horse battery".toCharArray()
        val wrongPassword = "incorrect horse battery".toCharArray()
        try {
            val identity = WalletCrypto.deriveIdentity(seed, 1_700_000_000_000L)
            val encrypted = WalletCrypto.encryptSeed(seed, password, identity.address)
            assertEquals(WALLET_KDF_ITERATIONS, encrypted.iterations)
            val record = StoredWallet(
                version = 1,
                address = identity.address,
                publicKeyHex = identity.publicKeyHex,
                createdAtMillis = identity.createdAtMillis,
                updatedAtMillis = identity.createdAtMillis,
                encryptedSeedBase64 = android.util.Base64.encodeToString(encrypted.ciphertext, android.util.Base64.NO_WRAP),
                saltBase64 = android.util.Base64.encodeToString(encrypted.salt, android.util.Base64.NO_WRAP),
                ivBase64 = android.util.Base64.encodeToString(encrypted.iv, android.util.Base64.NO_WRAP),
                iterations = encrypted.iterations,
            )
            val decrypted = WalletCrypto.decryptSeed(record, password)
            try {
                assertArrayEquals(seed, decrypted)
            } finally {
                decrypted.fill(0)
            }
            runCatching { WalletCrypto.decryptSeed(record, wrongPassword) }
                .onSuccess { error("Wrong password unexpectedly decrypted the seed.") }

            val changedAddress = record.copy(address = "0".repeat(64))
            val tamperedResult = runCatching { WalletCrypto.decryptSeed(changedAddress, password) }
            assertTrue(tamperedResult.isFailure)
            tamperedResult.getOrNull()?.fill(0)
            encrypted.ciphertext.fill(0)
            encrypted.salt.fill(0)
            encrypted.iv.fill(0)
        } finally {
            seed.fill(0)
            password.fill('\u0000')
            wrongPassword.fill('\u0000')
        }
    }

    @Test
    fun passwordBytesAndWalletCryptoOperationsCanRunOffMainThread() {
        val executor = Executors.newSingleThreadExecutor()
        try {
            val result = executor.submit<String> {
                val seed = WalletCrypto.deriveSeed(context, phrase)
                try { WalletCrypto.toHex(seed) } finally { seed.fill(0) }
            }.get()
            assertEquals("86b21c2b9e5d6e4f6d71f324da1db33565ece8c86448a1eb1592a5271aae2847", result)
        } finally {
            executor.shutdownNow()
        }
    }
}
