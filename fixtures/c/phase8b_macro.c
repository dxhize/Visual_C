#include <stdlib.h>
#define DISPOSE(ptr) free(ptr)
int main(void) {
    void *first = malloc(4);
    void *second = malloc(8);
    if (!first || !second) return 2;
    free(first); DISPOSE(second);
    return 0;
}
