int global_value = 10;
static int file_value = 20;
int *global_ptr = &global_value;
static int update(void) {
    static int calls = 0;
    calls++;
    global_value += calls;
    return calls;
}
int main(void) {
    update();
    update();
    return global_value == 13 && file_value == 20 ? 0 : 1;
}
