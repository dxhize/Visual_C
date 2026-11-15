#include <stdlib.h>
#include <string.h>
int main(void) {
    char *a = malloc(8);
    if (!a) return 2;
    memset(a, 'A', 8);
    char *b = realloc(a, 32);
    if (!b) { free(a); return 2; }
    b[31] = 'Z';
    int ok = (b[0] == 'A');
    char *c = realloc(b, 4);
    if (!c) { free(b); return 2; }
    c[3] = 'B';
    free(c);
    return ok ? 0 : 1;
}
