#include <stdint.h>
#include <stdlib.h>

static volatile unsigned int visits;

__attribute__((noinline)) static void observe(unsigned char *candidate)
{
    (void)candidate;
    visits++;
}

int main(void)
{
    unsigned char *block = malloc(256);
    unsigned char *guard = malloc(32);
    if (!block || !guard) return 1;
    block[0] = 0x4b;
    observe(block);

    unsigned char *same = realloc(block, 128);
    if (!same) { free(block); free(guard); return 2; }
    observe(same);

    volatile size_t impossible = SIZE_MAX;
    unsigned char *failed = realloc(same, impossible);
    if (failed) { free(failed); free(guard); return 3; }
    observe(same);

    unsigned char *moved = realloc(same, 1024 * 1024);
    if (!moved) { free(same); free(guard); return 4; }
    observe(moved);
    if (moved[0] != 0x4b) return 5;
    free(moved);
    free(guard);

    unsigned char *zeroed = malloc(32);
    if (!zeroed) return 6;
    observe(zeroed);
    unsigned char *zero_result = realloc(zeroed, 0);
    if (zero_result) free(zero_result);

    void *volatile null_pointer = NULL;
    unsigned char *fresh = realloc(null_pointer, 24);
    if (!fresh) return 7;
    free(fresh);

    unsigned char *zero_size = malloc(0);
    free(zero_size);

    unsigned char *failed_malloc = malloc(impossible);
    if (failed_malloc) { free(failed_malloc); return 8; }
    unsigned char *overflow = calloc(impossible, 2);
    if (overflow) { free(overflow); return 9; }
    return 0;
}
