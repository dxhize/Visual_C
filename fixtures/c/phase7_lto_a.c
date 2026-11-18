#include "phase7_lto_shared.h"

int phase7_shared_value = 7;

int phase7_helper(int x) { return x + phase7_shared_value; }

int phase7_call_inline(int x) { return phase7_inline_helper(x); }
