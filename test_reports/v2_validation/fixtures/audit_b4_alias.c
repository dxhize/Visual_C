#include <stdlib.h>
int main(void) {
    int *p = malloc(2 * sizeof(int));
    if (!p) return 2;
    int *q = p + 1;
    *p = 5;
    *q = 9;
    int s = *p + *q;
    free(p);
    return s == 14 ? 0 : 1;
}
