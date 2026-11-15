int main(void) {
    volatile int total = 0;
    for (int i = 0; i < 12; ++i) {
        total += i;
    }
    return total == 66 ? 0 : 1;
}
