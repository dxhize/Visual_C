/* Realloc demonstration: grow a block, verify the old id chain survives,
 * then shrink and free. Exercises the realloc event pair. */
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    size_t cap = 4;
    int *data = malloc(cap * sizeof(int));
    if (data == 0) {
        return 1;
    }
    for (size_t i = 0; i < cap; i++) {
        data[i] = (int)i;
    }
    cap = 8;
    int *grown = realloc(data, cap * sizeof(int));
    if (grown == 0) {
        free(data);
        return 1;
    }
    for (size_t i = 4; i < cap; i++) {
        grown[i] = (int)(i * 11);
    }
    int *shrunk = realloc(grown, 2 * sizeof(int));
    if (shrunk == 0) {
        free(grown);
        return 1;
    }
    printf("%d %d\n", shrunk[0], shrunk[1]);
    free(shrunk);
    return 0;
}
