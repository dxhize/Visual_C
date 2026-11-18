int left = 11;
int right = 22;
int *slots[2] = {&left, &right};
int *scalar = &left;

static int *owner(void) {
    static int persistent = 41;
    persistent += 1;
    return &persistent;
}

int main(void) {
    left += 1;
    scalar = owner();
    right += 1;
    scalar = &right;
    return *scalar == 23 ? 0 : 1;
}
