#include <stdlib.h>
int main(void) {
    int *p = malloc(sizeof(int));
    if (!p) return 2;
    *p = 3;
    free(p);
    free(p);
    return 0;
}
