static volatile int observed;

static int observe(int *incoming, int *current) {
    *incoming += 1;
    *current += 2;
    observed = *incoming + *current;
    return observed;
}

static int recurse(int depth, int *incoming) {
    int local = 100 + depth;
    int shadow = 10 + depth;
    {
        int shadow = 20 + depth;
        observed = observe(incoming, &shadow);
    }
    if (depth > 0) {
        observed = recurse(depth - 1, &local);
    }
    return local + shadow;
}

int main(void) {
    int owner = 7;
    observed = recurse(2, &owner);
    observed = recurse(2, &owner);
    return owner == 9 ? 0 : 1;
}
