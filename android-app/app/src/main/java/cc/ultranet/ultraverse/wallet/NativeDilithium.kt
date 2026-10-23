package cc.ultranet.ultraverse.wallet

/** Native bridge to the same PQClean Dilithium-5 clean sources used by the website and node. */
internal object NativeDilithium {
    init {
        System.loadLibrary("ultranetcrypto")
    }

    external fun sha3_256(input: ByteArray): ByteArray
    external fun publicKey(seed: ByteArray): ByteArray
    external fun sign(seed: ByteArray, message: ByteArray): ByteArray
    external fun verify(publicKey: ByteArray, message: ByteArray, signature: ByteArray): Boolean
}
