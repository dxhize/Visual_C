#include <stdlib.h>

static volatile unsigned char observed;

int main(void) {
    unsigned char *first = malloc(24);
    unsigned char *second = malloc(40);
    unsigned char *third = malloc(80);
    if (!first || !second || !third) {
        free(first);
        free(second);
        free(third);
        return 1;
    }
    first[0] = 11;
    second[0] = 22;
    third[0] = 33;
    observed = first[0] + second[0] + third[0];
    first[0] = 44;
    observed = first[0];
    free(first);
    free(second);
    free(third);
    return 0;
}
