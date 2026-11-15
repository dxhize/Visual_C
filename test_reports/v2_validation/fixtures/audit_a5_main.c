#include <stdio.h>
extern int helper_compute(int base);
int main(void) {
    int v = helper_compute(40);
    printf("%d\n", v);
    return v == 42 ? 0 : 1;
}
