#include "phase7_lto_shared.h"

int phase7_use_all(int x) {
    return phase7_helper(x) + phase7_call_inline(x) + phase7_shared_value;
}
