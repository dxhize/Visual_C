/* The inferior is healthy but spends longer than the per-stop wait in a syscall. */
#include <unistd.h>
int main(void) {
    sleep(14);
    return 0;
}
