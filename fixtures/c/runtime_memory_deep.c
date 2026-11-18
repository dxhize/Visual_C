__attribute__((noinline)) static int deep(int depth)
{
    int local = depth;
    if (depth == 0)
        return local;
    return local + deep(depth - 1);
}

int main(void)
{
    return deep(205) != 21115;
}
