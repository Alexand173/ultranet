#include <stddef.h>
#include <stdint.h>
#include "pqclean/crypto_sign/dilithium5/clean/api.h"

void ultra_set_seed(const uint8_t *seed);

uint32_t ultra_public_key_bytes(void) {
    return PQCLEAN_DILITHIUM5_CLEAN_CRYPTO_PUBLICKEYBYTES;
}

uint32_t ultra_secret_key_bytes(void) {
    return PQCLEAN_DILITHIUM5_CLEAN_CRYPTO_SECRETKEYBYTES;
}

uint32_t ultra_signature_bytes(void) {
    return PQCLEAN_DILITHIUM5_CLEAN_CRYPTO_BYTES;
}

int ultra_keypair(const uint8_t *seed, uint8_t *public_key, uint8_t *secret_key) {
    ultra_set_seed(seed);
    return PQCLEAN_DILITHIUM5_CLEAN_crypto_sign_keypair(public_key, secret_key);
}

int ultra_sign(const uint8_t *message, size_t message_length, const uint8_t *secret_key, uint8_t *signature) {
    size_t signature_length = 0;
    return PQCLEAN_DILITHIUM5_CLEAN_crypto_sign_signature(
        signature,
        &signature_length,
        message,
        message_length,
        secret_key
    );
}

int ultra_verify(const uint8_t *signature, size_t signature_length, const uint8_t *message, size_t message_length, const uint8_t *public_key) {
    return PQCLEAN_DILITHIUM5_CLEAN_crypto_sign_verify(
        signature,
        signature_length,
        message,
        message_length,
        public_key
    );
}
