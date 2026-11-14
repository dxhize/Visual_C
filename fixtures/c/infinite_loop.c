/* Infinite loop: exercises the wall timeout and the step cap; the engine
 * must preserve partial snapshots and terminate the inferior tree. */
#include <stdio.h>

int main(void) {
    volatile unsigned counter = 0;
    while (1) {
        counter++;
        if (counter == 4294967295u) {
            printf("never happens\n");
        }
    }
    return 0;
}
