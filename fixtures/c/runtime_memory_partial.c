static volatile unsigned int observed;

static void hold(int *owner) {
    int values[3] = {41, 42, 43};
#ifdef RUNTIME_MEMORY_FAULT
    volatile int *bad = 0;
    observed = (unsigned int)(values[0] + *owner);
    *bad = values[1];
#else
    while (*owner == 7) {
        observed += (unsigned int)values[0];
    }
#endif
}

int main(void) {
    int owner = 7;
    hold(&owner);
    return 0;
}
