#define _GNU_SOURCE
#include <sys/mman.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int main(int argc, char **argv) {
    // Deterministic OS mapping across independent PIDs; this is not malloc
    // provenance and must never be used to equate GDB/Valgrind lifetimes.
    uintptr_t addr = (argc > 1 && argv[1][0] == 'a') ? 0x500000100000ULL : 0x500000000000ULL;
    void *wanted = (void *)addr;
    void *mapped = mmap(wanted, 4096, PROT_READ | PROT_WRITE,
                        MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE, -1, 0);
    if (mapped == MAP_FAILED || mapped != wanted) return 8;
    printf("pid=%ld mapped=%p\n", (long)getpid(), mapped);
    fflush(stdout);
    volatile char *p = malloc(8);
    if (!p) return 9;
    p[0] = 4;
    free((void *)p);
    // Valgrind reports this diagnostic from its independent execution.
    volatile int observed = p[0];
    (void)observed;
    return 0;
}
