package cc.ultranet.ultraverse.wallet

import android.content.Context
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.CharBuffer
import java.nio.charset.StandardCharsets
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Locale
import javax.crypto.Cipher
import javax.crypto.Mac
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

internal object WalletCrypto {
    private val secureRandom = SecureRandom()
    private val derivationDomain = "UltraNet/Dilithium5/keygen/v1".toByteArray(StandardCharsets.UTF_8)

    fun normalizePhrase(phrase: String): String = phrase.trim()
        .lowercase(Locale.ROOT)
        .split(Regex("\\s+"))
        .filter(String::isNotEmpty)
        .joinToString(" ")

    fun isValidRecoveryPhrase(context: Context, phrase: String): Boolean = runCatching {
        phraseWordsToEntropy(context, normalizePhrase(phrase)).fill(0)
        true
    }.getOrDefault(false)

    fun createRecoveryPhrase(context: Context): String {
        val entropy = ByteArray(16).also(secureRandom::nextBytes)
        val checksum = MessageDigest.getInstance("SHA-256").digest(entropy)
        val indexes = IntArray(12)
        for (word in indexes.indices) {
            var index = 0
            for (bitOffset in 0 until 11) {
                val bitIndex = word * 11 + bitOffset
                val bit = if (bitIndex < 128) {
                    (entropy[bitIndex / 8].toInt() ushr (7 - bitIndex % 8)) and 1
                } else {
                    (checksum[0].toInt() ushr (7 - (bitIndex - 128))) and 1
                }
                index = (index shl 1) or bit
            }
            indexes[word] = index
        }
        val words = loadWordList(context)
        val phrase = indexes.joinToString(" ") { words[it] }
        entropy.fill(0)
        checksum.fill(0)
        indexes.fill(0)
        return phrase
    }

    fun deriveSeed(context: Context, phrase: String): ByteArray {
        val normalized = normalizePhrase(phrase)
        val entropy = phraseWordsToEntropy(context, normalized)
        entropy.fill(0)
        val phraseBytes = normalized.toByteArray(StandardCharsets.UTF_8)
        val bip39Seed = try {
            pbkdf2(phraseBytes, "mnemonic".toByteArray(StandardCharsets.UTF_8), 2_048, 64, "HmacSHA512")
        } finally {
            phraseBytes.fill(0)
        }
        val domainInput = derivationDomain + bip39Seed
        return try {
            NativeDilithium.sha3_256(domainInput).also { require(it.size == WALLET_SEED_BYTES) }
        } finally {
            bip39Seed.fill(0)
            domainInput.fill(0)
        }
    }

    fun deriveIdentity(seed: ByteArray, createdAtMillis: Long = System.currentTimeMillis()): WalletIdentity {
        require(seed.size == WALLET_SEED_BYTES) { "A wallet seed must contain 32 bytes." }
        val publicKey = NativeDilithium.publicKey(seed)
        require(publicKey.size == DILITHIUM_PUBLIC_KEY_BYTES) { "The local Dilithium-5 key has an incompatible public-key size." }
        val publicKeyDigest = NativeDilithium.sha3_256(publicKey)
        return try {
            val address = toHex(publicKeyDigest)
            WalletIdentity(address, toHex(publicKey), createdAtMillis)
        } finally {
            publicKey.fill(0)
            publicKeyDigest.fill(0)
        }
    }

    fun encryptSeed(seed: ByteArray, password: CharArray, address: String): EncryptedSeed {
        require(seed.size == WALLET_SEED_BYTES) { "A wallet seed must contain 32 bytes." }
        require(password.size >= 12) { "Use at least 12 characters for the wallet password." }
        val salt = ByteArray(WALLET_SALT_BYTES).also(secureRandom::nextBytes)
        val iv = ByteArray(WALLET_IV_BYTES).also(secureRandom::nextBytes)
        val passwordBytes = passwordToUtf8(password)
        val key = try {
            pbkdf2(passwordBytes, salt, WALLET_KDF_ITERATIONS, 32, "HmacSHA256")
        } finally {
            passwordBytes.fill(0)
        }
        val aad = walletAad(address)
        return try {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(128, iv))
            cipher.updateAAD(aad)
            EncryptedSeed(cipher.doFinal(seed), salt, iv, WALLET_KDF_ITERATIONS)
        } finally {
            key.fill(0)
            aad.fill(0)
        }
    }

    fun decryptSeed(record: StoredWallet, password: CharArray): ByteArray {
        validateStoredWallet(record)
        require(password.size >= 12) { "Use the wallet password to unlock this wallet." }
        val salt = decodeBase64(record.saltBase64)
        val iv = decodeBase64(record.ivBase64)
        val ciphertext = decodeBase64(record.encryptedSeedBase64)
        val passwordBytes = passwordToUtf8(password)
        val key = try {
            pbkdf2(passwordBytes, salt, record.iterations, 32, "HmacSHA256")
        } finally {
            passwordBytes.fill(0)
        }
        val aad = walletAad(record.address)
        return try {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(128, iv))
            cipher.updateAAD(aad)
            cipher.doFinal(ciphertext).also { require(it.size == WALLET_SEED_BYTES) }
        } catch (_: Exception) {
            throw IllegalArgumentException("That password did not unlock this wallet.")
        } finally {
            salt.fill(0)
            iv.fill(0)
            ciphertext.fill(0)
            key.fill(0)
            aad.fill(0)
        }
    }

    fun validateStoredWallet(record: StoredWallet) {
        require(record.version == WALLET_VERSION) { "This wallet uses an unsupported format version." }
        require(record.iterations == WALLET_KDF_ITERATIONS) { "This wallet's encryption parameters are invalid." }
        require(record.address.matches(Regex("^[0-9a-f]{64}$"))) { "The stored wallet address is invalid." }
        require(record.publicKeyHex.matches(Regex("^[0-9a-f]{5184}$"))) { "The stored wallet public key is invalid." }
        require(decodeBase64(record.saltBase64).size == WALLET_SALT_BYTES) { "The stored wallet salt is invalid." }
        require(decodeBase64(record.ivBase64).size == WALLET_IV_BYTES) { "The stored wallet IV is invalid." }
        require(decodeBase64(record.encryptedSeedBase64).size == 48) { "The stored wallet ciphertext is invalid." }
        require(record.createdAtMillis >= 0 && record.updatedAtMillis >= record.createdAtMillis) { "The stored wallet timestamps are invalid." }
    }

    fun publicKeyFromHex(value: String): ByteArray {
        require(value.length == DILITHIUM_PUBLIC_KEY_BYTES * 2 && value.all { it.digitToIntOrNull(16) != null })
        return ByteArray(value.length / 2) { index -> value.substring(index * 2, index * 2 + 2).toInt(16).toByte() }
    }

    fun toHex(bytes: ByteArray): String = buildString(bytes.size * 2) {
        bytes.forEach { byte -> append((byte.toInt() and 0xff).toString(16).padStart(2, '0')) }
    }

    fun decodeBase64(value: String): ByteArray = android.util.Base64.decode(value, android.util.Base64.NO_WRAP)

    private fun walletAad(address: String): ByteArray =
        "UltraNet/AndroidLocalWallet/v1|$address".toByteArray(StandardCharsets.UTF_8)

    private fun passwordToUtf8(password: CharArray): ByteArray {
        val buffer = StandardCharsets.UTF_8.newEncoder().encode(CharBuffer.wrap(password))
        return ByteArray(buffer.remaining()).also { bytes ->
            buffer.get(bytes)
            if (buffer.hasArray()) buffer.array().fill(0)
        }
    }

    private fun phraseWordsToEntropy(context: Context, phrase: String): ByteArray {
        val words = phrase.split(Regex("\\s+")).filter(String::isNotEmpty)
        require(words.size == 12) { "Enter a valid 12-word recovery phrase." }
        val wordList = loadWordList(context)
        val wordIndexes = wordList.withIndex().associate { it.value to it.index }
        val indexes = words.map { wordIndexes[it] ?: throw IllegalArgumentException("Enter a valid 12-word recovery phrase.") }
        val entropy = ByteArray(16)
        var checksumNibble = 0
        for (wordIndex in indexes.indices) {
            val index = indexes[wordIndex]
            for (bitOffset in 0 until 11) {
                val bitPosition = wordIndex * 11 + bitOffset
                val bit = (index ushr (10 - bitOffset)) and 1
                if (bitPosition < 128) {
                    if (bit == 1) entropy[bitPosition / 8] = (entropy[bitPosition / 8].toInt() or (1 shl (7 - bitPosition % 8))).toByte()
                } else {
                    checksumNibble = (checksumNibble shl 1) or bit
                }
            }
        }
        val hash = MessageDigest.getInstance("SHA-256").digest(entropy)
        val expected = (hash[0].toInt() ushr 4) and 0x0f
        hash.fill(0)
        require(checksumNibble == expected) {
            entropy.fill(0)
            "Enter a valid 12-word recovery phrase."
        }
        return entropy
    }

    private fun loadWordList(context: Context): List<String> = context.assets.open("bip39-english.txt").bufferedReader(StandardCharsets.UTF_8).use { reader ->
        reader.readLines().map(String::trim).filter(String::isNotEmpty).also { list ->
            check(list.size == 2_048) { "The local recovery word list is incomplete." }
        }
    }

    private fun pbkdf2(password: ByteArray, salt: ByteArray, iterations: Int, length: Int, algorithm: String): ByteArray {
        require(iterations > 0 && length in 1..64)
        val mac = Mac.getInstance(algorithm)
        mac.init(SecretKeySpec(password, algorithm))
        val blockInput = ByteArray(salt.size + 4)
        System.arraycopy(salt, 0, blockInput, 0, salt.size)
        ByteBuffer.wrap(blockInput, salt.size, 4).order(ByteOrder.BIG_ENDIAN).putInt(1)
        var u = mac.doFinal(blockInput)
        val result = u.copyOf()
        repeat(iterations - 1) {
            val next = mac.doFinal(u)
            u.fill(0)
            u = next
            for (index in result.indices) result[index] = (result[index].toInt() xor u[index].toInt()).toByte()
        }
        blockInput.fill(0)
        u.fill(0)
        if (result.size == length) return result
        return result.copyOf(length).also { result.fill(0) }
    }
}

internal data class EncryptedSeed(
    val ciphertext: ByteArray,
    val salt: ByteArray,
    val iv: ByteArray,
    val iterations: Int,
)
