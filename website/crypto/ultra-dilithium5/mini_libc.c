#include <stddef.h>

static size_t heap_cursor = 16 * 1024 * 1024;
static const size_t heap_limit = 64 * 1024 * 1024;

void *malloc(size_t size) {
    const size_t alignment = 16;
    size_t aligned = (size + alignment - 1) & ~(alignment - 1);
    if (aligned > heap_limit - heap_cursor) return 0;
    void *result = (void *)heap_cursor;
    heap_cursor += aligned;
    return result;
}

void free(void *pointer) {
    (void)pointer;
}

void ultra_reset_heap(void) {
    heap_cursor = 16 * 1024 * 1024;
}

void exit(int status) {
    (void)status;
    __builtin_trap();
}

void *memcpy(void *destination, const void *source, size_t length) {
    unsigned char *out = destination;
    const unsigned char *in = source;
    for (size_t index = 0; index < length; index++) out[index] = in[index];
    return destination;
}

void *memset(void *destination, int value, size_t length) {
    unsigned char *out = destination;
    for (size_t index = 0; index < length; index++) out[index] = (unsigned char)value;
    return destination;
}

int memcmp(const void *left, const void *right, size_t length) {
    const unsigned char *a = left;
    const unsigned char *b = right;
    for (size_t index = 0; index < length; index++) {
        if (a[index] != b[index]) return a[index] < b[index] ? -1 : 1;
    }
    return 0;
}
