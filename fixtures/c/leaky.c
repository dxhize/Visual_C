/* Leak demonstration: two blocks are allocated, one is freed, one is lost.
 * Valgrind reports a definite leak; heap_state still shows the live block. */
#include <stdlib.h>

struct Node {
    int id;
    struct Node *next;
};

int main(void) {
    struct Node *a = malloc(sizeof(struct Node));
    struct Node *b = malloc(sizeof(struct Node));
    if (a == 0 || b == 0) {
        return 1;
    }
    a->id = 1;
    a->next = 0;
    b->id = 2;
    b->next = 0;
    free(a); /* b leaks */
    return 0;
}
