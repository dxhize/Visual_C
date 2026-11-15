int main(void) {
    int total = 0;
    for (int i = 0; i < 20; ++i)
        for (int j = 0; j < 10; ++j)
            total += i * j;
    return total == 9025 ? 0 : 1;
}
