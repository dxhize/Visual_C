#include <stdlib.h>
int main(void) {
    int *p = malloc(16);
    if (!p) return 2;
    p[0] = 0x41424344;
    p[1] = 77;
    int sum = p[0] + p[1];
    free(p);
    return (sum != 0) ? 0 : 1;
}
