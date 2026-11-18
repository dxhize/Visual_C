#ifndef PHASE7_LTO_SHARED_H
#define PHASE7_LTO_SHARED_H

extern int phase7_shared_value;

int phase7_helper(int x);
int phase7_call_inline(int x);

static inline int phase7_inline_helper(int x) {
    return x * 3 + phase7_shared_value;
}

#endif
