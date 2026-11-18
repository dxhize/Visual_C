/* GCC DWARF v5 feasibility and layout fixture. */
typedef const unsigned long Count;
typedef int Five[5];
enum Color { RED = -2, GREEN = 4, BLUE = 4 };
struct Node {
    int value;
    char flag;
    struct Node *next;
    unsigned bits : 3;
    unsigned more : 5;
};
union Overlay { int n; char bytes[4]; };
struct Container {
    struct Node nodes[2];
    int grid[2][3];
    union Overlay overlay;
    int flexible[];
};
static int global_array[5] = {1, 2, 3, 4, 5};
static Five named_array;
static struct Node global_node;
static struct Container global_container;
static enum Color sample_color = RED;
typedef volatile int VolatileInt;
static VolatileInt volatile_counter;
static _Bool bool_flag;
static int read_ptr(int *restrict p) { return *p; }
static int func(int n) {
    int local[n];
    for (int i = 0; i < n; ++i) local[i] = i;
    return local[n - 1] + global_array[0];
}
int main(void) {
    Count c = 3;
    named_array[0] = global_array[0];
    global_node.value = func((int)c);
    global_container.overlay.n = global_node.value;
    volatile_counter = global_container.overlay.n;
    bool_flag = read_ptr(&global_node.value) == 3;
    return global_node.value != 3 || sample_color != RED ||
           global_container.overlay.n != 3 || !bool_flag;
}
