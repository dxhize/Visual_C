/* A fast terminal write: independent POSIX stdout length is exactly 4096. */
#include <unistd.h>
int main(void) {
    char data[4096];
    for (int i = 0; i < 4096; i++) data[i] = 'X';
    return write(1, data, sizeof(data)) == sizeof(data) ? 0 : 2;
}
