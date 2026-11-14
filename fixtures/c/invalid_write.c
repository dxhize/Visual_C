/* Invalid write: writes far past the end of a heap block. The overshoot is
 * large enough that the process reliably crashes (SIGSEGV) even without
 * sanitizers, exercising the crash + sanitizer-diagnostic path. */
#include <stdlib.h>

int main(void) {
    char *small = malloc(8);
    if (small == 0) {
        return 1;
    }
    for (int i = 0; i < 8; i++) {
        small[i] = (char)i;
    }
    /* Deliberate wild write far beyond the block. */
    char *far_end = small + 65536;
    *far_end = 'X';
    free(small);
    return 0;
}
