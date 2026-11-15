#include <stdint.h>
#include <stdlib.h>
#include <string.h>
int main(void) {
    size_t impossible = SIZE_MAX;
    void *zero = malloc(0);
    free(zero);
    void *failed = realloc(NULL, impossible);
    if (failed) { free(failed); return 1; }
    void *overflow = calloc(impossible, 2);
    if (overflow) { free(overflow); return 2; }
    /* strdup's internal allocation bypasses --wrap; tracking starts at realloc. */
    char *outside = strdup("value");
    if (!outside) return 3;
    char *resized = realloc(outside, 12);
    if (!resized) { free(outside); return 4; }
    int result = strcmp(resized, "value") == 0 ? 0 : 5;
    free(resized);
    return result;
}
