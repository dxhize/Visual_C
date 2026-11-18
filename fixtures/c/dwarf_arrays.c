struct Item { char label; int value; };
static struct Item objects[2];
static int *pointers[3];
static int matrix[2][3];
extern int incomplete[];
int incomplete[1];
int main(int argc, char **argv) {
    int dynamic[argc + 2];
    pointers[0] = &objects[0].value;
    matrix[0][0] = argc;
    dynamic[0] = matrix[0][0];
    incomplete[0] = dynamic[0];
    return pointers[0] == &objects[0].value && argc > 0 ? 0 : 1;
}
