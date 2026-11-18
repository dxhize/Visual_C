int main(void) {
    int value = ({ int inner = 2; inner + 1; });
    return value - 3;
}
