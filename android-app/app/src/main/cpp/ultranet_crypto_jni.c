#include <jni.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>

#include "api.h"
#include "fips202.h"

#define ULTRA_PUBLIC_KEY_BYTES 2592
#define ULTRA_SECRET_KEY_BYTES 4896
#define ULTRA_SIGNATURE_BYTES 4627
#define ULTRA_SEED_BYTES 32

extern void ultra_set_seed(const uint8_t *seed);
extern void ultra_clear_seed(void);
extern int ultra_keypair(const uint8_t *seed, uint8_t *public_key, uint8_t *secret_key);
extern int ultra_sign(const uint8_t *message, size_t message_length, const uint8_t *secret_key, uint8_t *signature);
extern int ultra_verify(const uint8_t *signature, size_t signature_length, const uint8_t *message, size_t message_length, const uint8_t *public_key);

static pthread_mutex_t crypto_mutex = PTHREAD_MUTEX_INITIALIZER;

static void wipe_bytes(uint8_t *value, size_t length) {
    volatile uint8_t *cursor = value;
    while (length-- > 0) *cursor++ = 0;
}

static jbyteArray native_sha3_256(JNIEnv *env, jobject receiver, jbyteArray input) {
    (void)receiver;
    if (input == NULL) return NULL;
    const jsize input_length = (*env)->GetArrayLength(env, input);
    uint8_t *source = (uint8_t *)malloc((size_t)(input_length > 0 ? input_length : 1));
    if (source == NULL) return NULL;
    if (input_length > 0) {
        (*env)->GetByteArrayRegion(env, input, 0, input_length, (jbyte *)source);
        if ((*env)->ExceptionCheck(env)) {
            wipe_bytes(source, (size_t)input_length);
            free(source);
            return NULL;
        }
    }
    uint8_t digest[32];
    sha3_256(digest, source, (size_t)input_length);
    jbyteArray output = (*env)->NewByteArray(env, 32);
    if (output != NULL) (*env)->SetByteArrayRegion(env, output, 0, 32, (const jbyte *)digest);
    wipe_bytes(digest, sizeof(digest));
    wipe_bytes(source, (size_t)(input_length > 0 ? input_length : 1));
    free(source);
    return output;
}

static jbyteArray native_public_key(JNIEnv *env, jobject receiver, jbyteArray seed_array) {
    (void)receiver;
    if (seed_array == NULL || (*env)->GetArrayLength(env, seed_array) != ULTRA_SEED_BYTES) return NULL;
    uint8_t seed[ULTRA_SEED_BYTES] = {0};
    uint8_t public_key[ULTRA_PUBLIC_KEY_BYTES] = {0};
    uint8_t secret_key[ULTRA_SECRET_KEY_BYTES] = {0};
    (*env)->GetByteArrayRegion(env, seed_array, 0, ULTRA_SEED_BYTES, (jbyte *)seed);
    if ((*env)->ExceptionCheck(env)) goto failure;

    jbyteArray result = NULL;
    pthread_mutex_lock(&crypto_mutex);
    const int status = ultra_keypair(seed, public_key, secret_key);
    ultra_clear_seed();
    pthread_mutex_unlock(&crypto_mutex);
    if (status != 0) goto failure;

    result = (*env)->NewByteArray(env, ULTRA_PUBLIC_KEY_BYTES);
    if (result != NULL) (*env)->SetByteArrayRegion(env, result, 0, ULTRA_PUBLIC_KEY_BYTES, (const jbyte *)public_key);
    wipe_bytes(seed, sizeof(seed));
    wipe_bytes(public_key, sizeof(public_key));
    wipe_bytes(secret_key, sizeof(secret_key));
    return result;

failure:
    ultra_clear_seed();
    wipe_bytes(seed, sizeof(seed));
    wipe_bytes(public_key, sizeof(public_key));
    wipe_bytes(secret_key, sizeof(secret_key));
    return NULL;
}

static jbyteArray native_sign(JNIEnv *env, jobject receiver, jbyteArray seed_array, jbyteArray message_array) {
    (void)receiver;
    if (seed_array == NULL || message_array == NULL || (*env)->GetArrayLength(env, seed_array) != ULTRA_SEED_BYTES) return NULL;
    const jsize message_length = (*env)->GetArrayLength(env, message_array);
    if (message_length <= 0 || message_length > 4096) return NULL;

    uint8_t seed[ULTRA_SEED_BYTES] = {0};
    uint8_t public_key[ULTRA_PUBLIC_KEY_BYTES] = {0};
    uint8_t secret_key[ULTRA_SECRET_KEY_BYTES] = {0};
    uint8_t signature[ULTRA_SIGNATURE_BYTES] = {0};
    uint8_t *message = (uint8_t *)malloc((size_t)message_length);
    if (message == NULL) return NULL;
    (*env)->GetByteArrayRegion(env, seed_array, 0, ULTRA_SEED_BYTES, (jbyte *)seed);
    (*env)->GetByteArrayRegion(env, message_array, 0, message_length, (jbyte *)message);
    if ((*env)->ExceptionCheck(env)) goto failure;

    pthread_mutex_lock(&crypto_mutex);
    int status = ultra_keypair(seed, public_key, secret_key);
    ultra_clear_seed();
    if (status == 0) status = ultra_sign(message, (size_t)message_length, secret_key, signature);
    pthread_mutex_unlock(&crypto_mutex);
    if (status != 0) goto failure;

    jbyteArray result = (*env)->NewByteArray(env, ULTRA_SIGNATURE_BYTES);
    if (result != NULL) (*env)->SetByteArrayRegion(env, result, 0, ULTRA_SIGNATURE_BYTES, (const jbyte *)signature);
    wipe_bytes(seed, sizeof(seed));
    wipe_bytes(public_key, sizeof(public_key));
    wipe_bytes(secret_key, sizeof(secret_key));
    wipe_bytes(signature, sizeof(signature));
    wipe_bytes(message, (size_t)message_length);
    free(message);
    return result;

failure:
    ultra_clear_seed();
    wipe_bytes(seed, sizeof(seed));
    wipe_bytes(public_key, sizeof(public_key));
    wipe_bytes(secret_key, sizeof(secret_key));
    wipe_bytes(signature, sizeof(signature));
    wipe_bytes(message, (size_t)message_length);
    free(message);
    return NULL;
}

static jboolean native_verify(JNIEnv *env, jobject receiver, jbyteArray public_key_array, jbyteArray message_array, jbyteArray signature_array) {
    (void)receiver;
    if (public_key_array == NULL || message_array == NULL || signature_array == NULL) return JNI_FALSE;
    if ((*env)->GetArrayLength(env, public_key_array) != ULTRA_PUBLIC_KEY_BYTES ||
        (*env)->GetArrayLength(env, signature_array) != ULTRA_SIGNATURE_BYTES) return JNI_FALSE;
    const jsize message_length = (*env)->GetArrayLength(env, message_array);
    if (message_length <= 0 || message_length > 4096) return JNI_FALSE;

    uint8_t public_key[ULTRA_PUBLIC_KEY_BYTES];
    uint8_t signature[ULTRA_SIGNATURE_BYTES];
    uint8_t *message = (uint8_t *)malloc((size_t)message_length);
    if (message == NULL) return JNI_FALSE;
    (*env)->GetByteArrayRegion(env, public_key_array, 0, ULTRA_PUBLIC_KEY_BYTES, (jbyte *)public_key);
    (*env)->GetByteArrayRegion(env, signature_array, 0, ULTRA_SIGNATURE_BYTES, (jbyte *)signature);
    (*env)->GetByteArrayRegion(env, message_array, 0, message_length, (jbyte *)message);
    if ((*env)->ExceptionCheck(env)) {
        wipe_bytes(public_key, sizeof(public_key));
        wipe_bytes(signature, sizeof(signature));
        wipe_bytes(message, (size_t)message_length);
        free(message);
        return JNI_FALSE;
    }

    pthread_mutex_lock(&crypto_mutex);
    const int status = ultra_verify(signature, ULTRA_SIGNATURE_BYTES, message, (size_t)message_length, public_key);
    pthread_mutex_unlock(&crypto_mutex);
    wipe_bytes(public_key, sizeof(public_key));
    wipe_bytes(signature, sizeof(signature));
    wipe_bytes(message, (size_t)message_length);
    free(message);
    return status == 0 ? JNI_TRUE : JNI_FALSE;
}

static JNINativeMethod methods[] = {
    {"sha3_256", "([B)[B", (void *)native_sha3_256},
    {"publicKey", "([B)[B", (void *)native_public_key},
    {"sign", "([B[B)[B", (void *)native_sign},
    {"verify", "([B[B[B)Z", (void *)native_verify},
};

JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env = NULL;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK) return JNI_ERR;
    jclass target = (*env)->FindClass(env, "cc/ultranet/ultraverse/wallet/NativeDilithium");
    if (target == NULL) return JNI_ERR;
    if ((*env)->RegisterNatives(env, target, methods, (jint)(sizeof(methods) / sizeof(methods[0]))) != JNI_OK) return JNI_ERR;
    return JNI_VERSION_1_6;
}
