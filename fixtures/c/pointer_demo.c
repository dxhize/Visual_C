/* Pointer demonstration: own address vs. pointed-to address, writes through
 * pointers, address-of locals, pointer arithmetic on heap memory. */
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    int value = 42;
    int *p = &value;
    int *heap = malloc(4 * sizeof(int));
    if (heap == 0) {
        return 1;
    }
    for (int i = 0; i < 4; i++) {
        heap[i] = i * 10;
    }
    int *third = heap + 2;
    *p = *third + 2; /* value becomes 22 */
    printf("value=%d heap[3]=%d\n", value, heap[3]);
    free(heap);
    return 0;
}
