static int leaf(int x) { return x + 1; }
static int middle(int y) { return leaf(y * 2) + leaf(y); }
int main(void) {
    int r = middle(10);
    r += middle(3);
    return r == 43 ? 0 : 1;
}
