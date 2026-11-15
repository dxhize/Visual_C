#include <stdlib.h>
int main(void) {
    long total = 0;
    for (int i = 0; i < 50; ++i) {
        int *p = malloc(32);
        if (!p) return 2;
        p[0] = i;
        p[7] = i * 2;
        total += p[0] + p[7];
        free(p);
    }
    return total == 3675 ? 0 : 1;
}
