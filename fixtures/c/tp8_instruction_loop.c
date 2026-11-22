/* TP-8 audit-only: bounded high-retained-instruction workload.
 * With --trace-mode dual --max-steps 900, the engine retains thousands of
 * real instruction events without needing a memory-unbounded C loop. */
int main(void) {
    volatile int sum = 0;
    for (int i = 0; i < 4000; ++i) {
        sum += i;
    }
    return sum == -1;
}
