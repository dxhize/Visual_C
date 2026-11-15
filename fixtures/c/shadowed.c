int value = 10;
int main(void) {
    int value = 20;
    {
        int value = 30;
        value++;
    }
    return value == 20 ? 0 : 1;
}
