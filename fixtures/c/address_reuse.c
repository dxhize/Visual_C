#include <stdlib.h>
int main(void) {
    char *a = malloc(32);
    free(a);
    char *b = malloc(32);
    if (!b) return 1;
    b[0] = 1;
    free(b);
    return 0;
}
