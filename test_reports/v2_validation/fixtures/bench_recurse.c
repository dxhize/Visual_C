static int down(int n) {
    if (n <= 0) return 0;
    return 1 + down(n - 1);
}
int main(void) {
    int total = down(50);
    return total == 50 ? 0 : 1;
}
