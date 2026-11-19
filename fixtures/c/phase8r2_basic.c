#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static int worker_iterations = 20;

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
    for (int i = 0; i < worker_iterations; i++)
        allocation_pair();
    return NULL;
}

int main(int argc, char **argv)
{
    if (argc < 2 || argc > 3)
        return 2;
    int iterations = 1;
    if (argc == 3) {
        char *end = NULL;
        unsigned long parsed = strtoul(argv[2], &end, 10);
        if (!end || *end || parsed == 0 || parsed > 1000000)
            return 8;
        iterations = (int)parsed;
    }
    if (strcmp(argv[1], "forks") == 0) {
        allocation_pair();
        for (int i = 0; i < (argc == 3 ? iterations : 3); i++) {
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
        if (argc == 3)
            worker_iterations = iterations;
        pthread_t threads[4];
        for (int i = 0; i < 4; i++)
            if (pthread_create(&threads[i], NULL, worker, NULL))
                return 5;
        for (int i = 0; i < 4; i++)
            if (pthread_join(threads[i], NULL))
                return 6;
    } else if (strcmp(argv[1], "normal") == 0) {
        for (int i = 0; i < iterations; i++)
            allocation_pair();
    } else {
        return 7;
    }
    return 0;
}
