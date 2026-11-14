/* Use-after-free: reads a field of a freed block. Valgrind reports an
 * invalid read; the program does NOT crash on most systems. */
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    int *p = malloc(sizeof(int));
    if (p == 0) {
        return 1;
    }
    *p = 7;
    free(p);
    int after = *p; /* invalid read of freed memory */
    printf("after=%d\n", after); /* suppress unused warnings */
    return 0;
}
