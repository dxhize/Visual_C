#include <stdlib.h>
static int compare(const void *a, const void *b) {
    int left = *(const int *)a;
    int right = *(const int *)b;
    return (left > right) - (left < right);
}
int main(void) {
    int values[3] = {3, 1, 2};
    qsort(values, 3, sizeof(int), compare);
    return values[0] == 1 ? 0 : 1;
}
