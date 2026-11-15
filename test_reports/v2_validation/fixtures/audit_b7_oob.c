#include <stdlib.h>
int main(void) {
    int *p = malloc(2 * sizeof(int));
    if (!p) return 2;
    p[2] = 7;
    int v = p[2];
    free(p);
    return v == 7 ? 0 : 1;
}
