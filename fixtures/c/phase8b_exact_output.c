#include <unistd.h>
int main(void) {
    static const char message[] = "0123456789abcdef";
    for (int i = 0; i < 4; ++i)
        if (write(1, message, sizeof(message) - 1) != sizeof(message) - 1)
            return 2;
    return 0;
}
