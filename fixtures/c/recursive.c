static int recurse(int value) {
    if (value == 0) return 1;
    int result = recurse(value - 1);
    return result + value;
}
int main(void) {
    int value = recurse(3);
    return value == 7 ? 0 : 1;
}
