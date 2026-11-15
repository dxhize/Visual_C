int main(void) {
    volatile int ready = 0;
    while (!ready) {}
    return 0;
}
