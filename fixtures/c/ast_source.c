#include <stdlib.h>
#define TAKE(N) malloc((N) * sizeof(int))
struct Point { int x; int y; };
static int local_counter;
int main(void) {
    int count = 4;
    int array[3] = { 1, 2, 3 };
    struct Point point = { 0, 0 };
    int *p = TAKE(count);
    int *q = calloc(count, sizeof(*q));
    int *r = malloc(8), *s = malloc(16);
    p = realloc(p, (count + 1) * sizeof(int));
    if (p) { p[1] = array[0]; point.x = p[1]; }
    int *next = p ? p + 1 : q;
    int *address = &array[1];
    int value = *address;
    { int value = 3; local_counter += value; }
    free(p); free(q); free(r); free(s);
    return (next == 0) + point.x - 1 + value - 2 + local_counter - 3;
}
