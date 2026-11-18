extern int phase8r1_read_a(void);
extern int phase8r1_read_b(void);

int main(void) {
    return phase8r1_read_a() != 11 || phase8r1_read_b() != 22;
}
