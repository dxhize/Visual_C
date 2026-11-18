/* V2.2 Phase 1 scope-liveness scenarios: nested lexical scopes (E),
 * shadowing (F), recursion with distinct activations (G), and stack
 * address reuse across sequential activations (H). */

static int helper(int depth, int **captured) {
    int local = depth * 10;
    if (depth == 0) {
        *captured = &local;
        return local;
    }
    return helper(depth - 1, captured);
}

int main(void) {
    int x = 10;
    int r = 0;
    int *self = 0;
    {
        int b = 20;
        int x = 1;
        int *p = &x;
        p = &b;
        if (p == 0) {
            return 1;
        }
        r = helper(3, &self);
        r = helper(2, &self);
    }
    return x + r;
}
