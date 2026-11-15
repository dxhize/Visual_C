#include <stdio.h>
int main(void) {
    for (int i = 0; i < 2000; ++i)
        printf("line %06d padding padding padding padding\n", i);
    return 0;
}
