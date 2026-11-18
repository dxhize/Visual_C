static int shared = 20;
struct Node { double value; };
static struct Node marker;

int bump_other(void) {
    shared += 2;
    marker.value = shared;
    return shared;
}
