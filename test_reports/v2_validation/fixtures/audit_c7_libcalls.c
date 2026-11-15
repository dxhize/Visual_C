#include <stdio.h>
#include <stdlib.h>
static int use(int *p) { return p ? *p : -1; }
int main(void) {
    int *p = malloc(sizeof(int));
    if (!p) return 2;
    *p = 41;
    int v = use(p) + 1;
    printf("v=%d\n", v);
    free(p);
    return v == 42 ? 0 : 1;
}
