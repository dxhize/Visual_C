/* Calloc demonstration: zero-initialized allocation of an array; the engine
 * records calloc as its own allocation type. */
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    double *samples = calloc(16, sizeof(double));
    if (samples == 0) {
        return 1;
    }
    for (int i = 0; i < 16; i++) {
        samples[i] = samples[i] + (double)i * 0.5;
    }
    printf("%f %f\n", samples[0], samples[15]);
    free(samples);
    return 0;
}
