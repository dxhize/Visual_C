#include <stdint.h>
#include <stdlib.h>

static volatile unsigned int visits;

__attribute__((noinline)) static void observe(unsigned char *candidate)
{
    /* Keep the pointer in both the callee and its caller without dereferencing it. */
    (void)candidate;
    visits++;
}

int main(void)
{
    unsigned char *first = malloc(32);
    if (!first) return 1;
    first[0] = 0x5a;
    observe(first);

    uintptr_t old_address = (uintptr_t)first;
    unsigned char *stale = first;
    free(first);
    observe(stale);

    unsigned char *second = malloc(32);
    if (!second) return 2;
    int reused = (uintptr_t)second == old_address;
    second[0] = 0x36;
    observe(stale);
    observe(second);

    unsigned char *zeros = calloc(8, 4);
    if (!zeros) return 3;
    observe(zeros);
    zeros[0] = 0x71;
    observe(zeros);
    free(zeros);
    free(second);

    void *volatile null_pointer = NULL;
    free(null_pointer);
    return reused ? 0 : 4;
}
