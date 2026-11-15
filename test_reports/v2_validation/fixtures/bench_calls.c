static int square(int x) { return x * x; }
int main(void) {
    int acc = 0;
    for (int i = 1; i <= 100; ++i) acc += square(i);
    return acc == 338350 ? 0 : 1;
}
