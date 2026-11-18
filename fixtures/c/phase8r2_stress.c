#include <errno.h>
#include <pthread.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

/* Only present in CTE_TESTING builds of alloc_tracker.c. */
extern void cte_test_hold_lock(int ready_fd, int release_fd);
extern void cte_test_fault_next_event(int mode);
extern void cte_test_fail_status_ack(void);

static int ready_pipe[2], release_pipe[2];

static void *hold_tracker_lock(void *unused)
{
    (void)unused;
    cte_test_hold_lock(ready_pipe[1], release_pipe[0]);
    return NULL;
}

static int allocation_pair(void)
{
    void *p = malloc(37);
    if (!p)
        return 1;
    free(p);
    return 0;
}

int main(int argc, char **argv)
{
    if (argc != 2)
        return 2;
    if (strcmp(argv[1], "lock_fork") == 0) {
        if (allocation_pair() || pipe(ready_pipe) || pipe(release_pipe))
            return 3;
        pthread_t worker;
        if (pthread_create(&worker, NULL, hold_tracker_lock, NULL))
            return 4;
        char byte;
        if (read(ready_pipe[0], &byte, 1) != 1)
            return 5;
        pid_t child = fork();
        if (child < 0)
            return 6;
        if (child == 0) {
            alarm(3); /* Watchdog also releases the parent from waitpid pre-fix. */
            if (allocation_pair())
                exit(7);
            exit(0);
        }
        if (write(release_pipe[1], "r", 1) != 1 || pthread_join(worker, NULL))
            return 8;
        int status;
        if (waitpid(child, &status, 0) != child || !WIFEXITED(status) || WEXITSTATUS(status))
            return 9;
        return allocation_pair();
    }

    void *p = malloc(37);
    if (!p)
        return 10;
    int fault = 0;
    if (strcmp(argv[1], "partial") == 0)
        fault = 1;
    else if (strcmp(argv[1], "eintr") == 0)
        fault = 2;
    else if (strcmp(argv[1], "error") == 0)
        fault = 3;
    else if (strcmp(argv[1], "zero") == 0)
        fault = 4;
    else if (strcmp(argv[1], "eintr_exhausted") == 0)
        fault = 5;
    else if (strcmp(argv[1], "partial_error") == 0)
        fault = 6;
    else if (strcmp(argv[1], "ack_failure") != 0 && strcmp(argv[1], "abnormal") != 0)
        return 11;
    if (fault)
        cte_test_fault_next_event(fault);
    errno = E2BIG;
    free(p);
    if (errno != E2BIG)
        return 12;
    if (strcmp(argv[1], "ack_failure") == 0)
        cte_test_fail_status_ack();
    if (strcmp(argv[1], "abnormal") == 0)
        _exit(0);
    return 0;
}
