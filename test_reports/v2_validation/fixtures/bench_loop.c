int main(void) {
    int acc = 0;
    for (int i = 0; i < 200; ++i) acc += i;
    return acc == 19900 ? 0 : 1;
}
