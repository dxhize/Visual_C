/* Multi-file project: main translation unit. */
#include <stdio.h>
#include "helper.h"

int main(void) {
    int base = 40;
    int total = add_two(base, 2);
    printf("total=%d\n", total);
    return report(total);
}
