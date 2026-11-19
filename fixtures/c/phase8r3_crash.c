/* A requested global must be accounted for even at a fatal stop. */
int observed_global = 123;
int main(void) { *(volatile int *)0 = observed_global; return 0; }
