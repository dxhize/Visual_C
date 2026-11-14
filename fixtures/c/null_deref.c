/* NULL dereference: the classic crash used to exercise SIGSEGV handling,
 * crash report capture and the separate sanitizer diagnostic pass. */
#include <stdio.h>

int main(void) {
    int *p = 0;
    int crash = *p; /* SIGSEGV */
    printf("%d\n", crash); /* never reached */
    return 0;
}
