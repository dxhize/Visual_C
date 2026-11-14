/* Large output: exercises the captured-output cap and truncation flags. */
#include <stdio.h>

int main(void) {
    for (int i = 0; i < 200000; i++) {
        printf("line %06d: c-trace-engine captures program output\n", i);
    }
    return 0;
}
