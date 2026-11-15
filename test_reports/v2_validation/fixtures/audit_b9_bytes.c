int main(void) {
    int known = 0x11223344;
    int arr[3] = {1, -1, 256};
    int total = known + arr[0] + arr[1] + arr[2];
    return total == (256 + 0x11223344) ? 0 : 1;
}
