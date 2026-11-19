#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include <string.h>

static void pair(void) {
    void *p = malloc(24);
    if (!p) _exit(9);
    free(p);
}

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    pair();
    if (strcmp(argv[1], "parent") != 0) {
        for (int i = 0; i < (strcmp(argv[1], "repeated") == 0 ? 3 : 1); ++i) {
            pid_t child = fork();
            if (child < 0) return 3;
            if (!child) {
                if (strcmp(argv[1], "silent") == 0) _exit(0);
                pair();
                if (strcmp(argv[1], "incomplete") == 0) _exit(0);
                exit(0);
            }
            int status = 0;
            if (waitpid(child, &status, 0) != child || !WIFEXITED(status) || WEXITSTATUS(status) != 0) return 4;
        }
    }
    pair();
    return 0;
}
