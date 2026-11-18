/* V2.2 Phase 1 evidence scenarios: pointer-to-variable identity (A),
 * pointer reassignment (B), aliases (C), null pointers (D), interior
 * pointers (I), heap targets (J), and a pointer to a global. */
#include <stdlib.h>

int g_value = 7;

int main(void) {
    int x = 10;
    int y = 20;
    int *p = &x;                     /* A: p -> x */
    int *q = p;                      /* C: alias -> x */
    int *nul = 0;                    /* D: null */
    p = &y;                          /* B: reassign -> y (q keeps x) */
    int a[3] = {10, 20, 30};
    int *mid = &a[1];                /* I: interior of a */
    int *gp = &g_value;              /* global target */
    int *hp = malloc(sizeof(int));   /* J: heap */
    if (hp == 0) {
        return 1;
    }
    *hp = 5;
    g_value = *p + *q + *hp;
    free(hp);
    return g_value + a[0];
}
