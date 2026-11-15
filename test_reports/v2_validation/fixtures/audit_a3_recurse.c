static int countdown(int n) {
    if (n <= 0) return 0;
    return 1 + countdown(n - 1);
}
int main(void) {
    int total = countdown(6);
    return total == 6 ? 0 : 1;
}
