/* Uninitialized read: branches on a value never written. Valgrind reports a
 * conditional-jump-or-move depends-on-uninitialised-value error. */
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    int *raw = malloc(sizeof(int));
    if (raw == 0) {
        return 1;
    }
    int guess = *raw; /* never initialized */
    if (guess > 100) {
        printf("large\n");
    } else {
        printf("small\n");
    }
    free(raw);
    return 0;
}
