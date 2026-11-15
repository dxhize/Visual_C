#include <stdio.h>
int main(void) {
    int a = 6;
    int b = 7;
    int product = a * b;
    int shifted = product << 2;
    printf("%d %d\n", product, shifted);
    return (product == 42 && shifted == 168) ? 0 : 1;
}
