/* Many allocations: exercises the allocation cap and heap-block caps; every
 * block is freed so the timeline shows churn. */
#include <stdlib.h>

int main(void) {
    for (int i = 0; i < 5000; i++) {
        char *p = malloc(64);
        if (p == 0) {
            return 1;
        }
        p[0] = (char)i;
        free(p);
    }
    return 0;
}
