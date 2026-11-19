/* Defined-behavior oracle: recursion, nested aggregates, aliasing and realloc. */
#include <stdio.h>
#include <stdlib.h>

struct Packet { int items[3]; struct { int value; } nested; };
int oracle_global = 41;

static int triangular(int n) {
    if (n == 0) return 0;
    return n + triangular(n - 1);
}

__attribute__((noinline))
static int checkpoint(struct Packet *packet, int *alias, int *heap, int sum) {
    volatile int value = packet->items[1] + *alias + heap[2] + sum + oracle_global;
    return value;
}

int main(void) {
    struct Packet packet = {{3, 7, 11}, {13}};
    int *alias = &packet.items[1];
    int *heap = malloc(3 * sizeof(int));
    if (!heap) return 2;
    heap[0] = 17; heap[1] = 19; heap[2] = 23;
    int *grown = realloc(heap, 4 * sizeof(int));
    if (!grown) { free(heap); return 3; }
    heap = grown; heap[3] = 29;
    int sum = triangular(4);
    int result = checkpoint(&packet, alias, heap, sum);
    printf("oracle global=%d alias=%d heap2=%d sum=%d result=%d\n",
           oracle_global, *alias, heap[2], sum, result);
    free(heap);
    return result == 88 ? 0 : 4;
}
