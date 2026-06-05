#include <stdio.h>
#include <stdlib.h>

typedef struct { int64_t* ptr; int64_t len; } slice;

int64_t must_read() __asm__("must_read");
void must_print(int64_t x) __asm__("must_print");
slice must_alloc(int64_t size) __asm__("must_alloc");

int64_t must_read() {
    int64_t x;
    scanf("%ld", &x);
    return x;
}

void must_print(int64_t x) {
    printf("%ld\n", x);
}

slice must_alloc(int64_t size) {
    int64_t* ptr = malloc(sizeof(int64_t) * size);
    slice s = { .ptr = ptr, .len = size };
    return s;
}
