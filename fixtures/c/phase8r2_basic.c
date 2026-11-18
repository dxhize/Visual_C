#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static void allocation_pair(void)
{
    void *p = malloc(37);
    if (!p)
        _exit(20);
    free(p);
}

static void *worker(void *unused)
{
    (void)unused;
    for (int i = 0; i < 20; i++)
        allocation_pair();
    return NULL;
}

int main(int argc, char **argv)
{
    if (argc != 2)
        return 2;
    if (strcmp(argv[1], "forks") == 0) {
        allocation_pair();
        for (int i = 0; i < 3; i++) {
            pid_t child = fork();
            if (child < 0)
                return 3;
            if (child == 0) {
                allocation_pair();
                exit(0);
            }
            int status;
            if (waitpid(child, &status, 0) != child || !WIFEXITED(status) || WEXITSTATUS(status))
                return 4;
        }
        allocation_pair();
    } else if (strcmp(argv[1], "threads") == 0) {
        pthread_t threads[4];
        for (int i = 0; i < 4; i++)
            if (pthread_create(&threads[i], NULL, worker, NULL))
                return 5;
        for (int i = 0; i < 4; i++)
            if (pthread_join(threads[i], NULL))
                return 6;
    } else if (strcmp(argv[1], "normal") == 0) {
        allocation_pair();
    } else {
        return 7;
    }
    return 0;
}
