/* Two translation units deliberately declare different static objects
 * with the same name. File qualification must remain part of the cache key. */
static int shared = 11;
struct Node { int value; };
static struct Node marker;
extern int bump_other(void);

int main(void) {
    int first = shared;
    int second = bump_other();
    shared = second + 1;
    marker.value = shared;
    return first == 11 && second == 22 && shared == 23 && marker.value == 23 ? 0 : 1;
}
