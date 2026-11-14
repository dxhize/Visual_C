/* Multi-file project: helper translation unit. */
#include "helper.h"
#include <stdlib.h>

int add_two(int a, int b) {
    return a + b;
}

int report(int value) {
    int *copy = malloc(sizeof(int));
    if (copy == 0) {
        return -1;
    }
    *copy = value;
    int result = *copy > 0 ? 0 : 1;
    free(copy);
    return result;
}
