#include <stddef.h>
#include <stdint.h>

static uint8_t deterministic_seed[32];
static int seed_ready = 0;

static void wipe_bytes(uint8_t *value, size_t length) {
    volatile uint8_t *cursor = value;
    while (length-- > 0) *cursor++ = 0;
}

void ultra_set_seed(const uint8_t *seed) {
    for (size_t index = 0; index < sizeof(deterministic_seed); index++) {
        deterministic_seed[index] = seed[index];
    }
    seed_ready = 1;
}

void ultra_clear_seed(void) {
    wipe_bytes(deterministic_seed, sizeof(deterministic_seed));
    seed_ready = 0;
}

int PQCLEAN_randombytes(uint8_t *output, size_t length) {
    if (!seed_ready || length > sizeof(deterministic_seed)) {
        ultra_clear_seed();
        return -1;
    }
    for (size_t index = 0; index < length; index++) output[index] = deterministic_seed[index];
    ultra_clear_seed();
    return 0;
}
