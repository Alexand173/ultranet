#include <stddef.h>
#include <stdint.h>

extern void *memcpy(void *destination, const void *source, size_t length);

static uint8_t deterministic_seed[32];
static int seed_ready = 0;

void ultra_set_seed(const uint8_t *seed) {
    memcpy(deterministic_seed, seed, sizeof(deterministic_seed));
    seed_ready = 1;
}

int PQCLEAN_randombytes(uint8_t *output, size_t length) {
    if (!seed_ready || length > sizeof(deterministic_seed)) {
        return -1;
    }
    memcpy(output, deterministic_seed, length);
    seed_ready = 0;
    return 0;
}
