#if defined(__OPTIMIZE__)
#define BRANCH_NAME optimized_branch
#else
#define BRANCH_NAME unoptimized_branch
#endif

int BRANCH_NAME = 1;

#ifdef PHASE8R1_FROM_DEFINE
int defined_branch = PHASE8R1_FROM_DEFINE;
#endif
#ifdef PHASE8R1_REMOVED
#error The -U option must remove this macro
#endif

int main(void) {
    return BRANCH_NAME - 1;
}
