static int file_static = 11;
int global_counter = 22;
static int bump(void) {
    static int calls = 0;
    calls += 1;
    global_counter += calls;
    return global_counter;
}
int main(void) {
    int *gp = &global_counter;
    bump();
    bump();
    return (*gp == 25 && file_static == 11) ? 0 : 1;
}
