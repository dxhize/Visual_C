/*
 * c-trace-engine sample program.
 *
 * Demonstrates the features the engine records:
 *   - multi-file-style stepping through a helper function
 *   - heap allocation, initialization, modification and free
 *   - pointer variables (own address vs. pointed-to address)
 *   - a struct on the stack
 *   - program output on stdout
 *
 * Run:  cargo run -- sample.c
 */
#include <stdio.h>
#include <stdlib.h>

struct Point {
    int x;
    int y;
};

static int sum_of_three(const int *values) {
    int total = 0;
    for (int i = 0; i < 3; i++) {
        total += values[i];
    }
    return total;
}

int main(void) {
    int *arr = malloc(3 * sizeof(int));
    if (arr == NULL) {
        return 1;
    }

    arr[0] = 10;
    arr[1] = 20;
    arr[2] = 30;

    struct Point p = {1, 2};
    int *ptr = arr;
    int sum = sum_of_three(arr);

    printf("sum=%d p.y=%d ptr[2]=%d\n", sum, p.y, ptr[2]);

    free(arr);
    return 0;
}
