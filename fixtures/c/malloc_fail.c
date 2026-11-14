/* Failed allocation: a huge malloc legitimately returns NULL; the program
 * must handle it. The engine records the failed allocation event. */
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    /* SIZE_MAX/2 is far beyond any real address space. */
    size_t huge = (size_t)-1 / 2;
    char *p = malloc(huge);
    if (p == 0) {
        printf("allocation failed as expected\n");
        return 0;
    }
    free(p);
    printf("unexpected success\n");
    return 0;
}
