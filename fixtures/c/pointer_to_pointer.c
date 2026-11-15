int main(void) {
    int value = 7;
    int *ptr = &value;
    int **link = &ptr;
    **link = 9;
    return value == 9 ? 0 : 1;
}
