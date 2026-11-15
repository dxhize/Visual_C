#include <pthread.h>
static void *work(void *arg) { return arg; }
int main(void) {
    pthread_t thread;
    if (pthread_create(&thread, 0, work, 0)) return 1;
    pthread_join(thread, 0);
    return 0;
}
