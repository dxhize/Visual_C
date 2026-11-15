#include <stdlib.h>
int main(void) {
    char *p = malloc(32);
    char *guard = malloc(32);
    if (!p || !guard) return 1;
    p[0] = 42;
    char *next = realloc(p, 1024 * 1024);
    if (!next) { free(p); free(guard); return 1; }
    int result = next[0] == 42 ? 0 : 1;
    free(next);
    free(guard);
    return result;
}
