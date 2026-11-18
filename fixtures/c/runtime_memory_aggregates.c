struct Pair {
    int x;
    int y;
};

union Overlay {
    unsigned int word;
    unsigned char bytes[4];
};

struct Bundle {
    struct Pair pair;
    int values[3];
    union Overlay overlay;
};

static struct Pair mutable_global = {1, 2};
static volatile int observed;

static void probe(int *value) {
    *value += 1;
    observed = *value;
}

static int capture(int n) {
    static struct Pair mutable_static = {3, 4};
    int anchor = 31;
    struct Pair pairs[2] = {{11, 12}, {13, 14}};
    int grid[2][3] = {{1, 2, 3}, {4, 5, 6}};
    int *pointers[2] = {&anchor, &pairs[1].x};
    union Overlay overlays[2] = {{.word = 0x11223344u}, {.word = 0x55667788u}};
    struct Bundle bundle = {{21, 22}, {23, 24, 25}, {.word = 26}};
    unsigned char large[20000] = {0x5a};
    int dynamic[n];
    dynamic[0] = n;
    dynamic[n - 1] = n + 1;
    mutable_global.x += n;
    mutable_static.y += n;
    probe(&anchor);
    observed = pairs[0].x + grid[1][2] + *pointers[0] + (int)overlays[0].bytes[0];
    return bundle.pair.x + dynamic[0] + dynamic[n - 1] + large[0];
}

int main(void) {
    observed = capture(3);
    observed = capture(5);
    return 0;
}
