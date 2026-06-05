#include <stdio.h>
#include <stdlib.h>

int64_t must_read() {
    int64_t x;
    scanf("%ld", &x);
    return x;
}

void must_print(int64_t x) {
    printf("%ld\n", x);
}

typedef struct { int64_t* ptr; int64_t len; } slice;

slice must_alloc(int64_t size) {
    int64_t* ptr = malloc(sizeof(int64_t) * size);
    slice s = { .ptr = ptr, .len = size };
    return s;
}
