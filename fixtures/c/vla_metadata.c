/* Runtime-sized storage must be observed at each activation, not memoized. */
static int consume(int n) {
    int items[n];
    for (int i = 0; i < n; ++i) {
        items[i] = i + n;
    }
    return items[n - 1];
}

int main(void) {
    int first = consume(3);
    int second = consume(5);
    return first == 5 && second == 9 ? 0 : 1;
}
