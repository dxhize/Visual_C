int main(void) {
    int acc = 0;
    for (int i = 1; i <= 5; ++i) acc += i;
    return acc == 15 ? 0 : 1;
}
