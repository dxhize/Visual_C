static int a(int x) { return x + 1; } static int b(int x) { return x * 2; }
int main(void) { int x = b(a(2)); return x == 6 ? 0 : 1; }
