/* V2.2 Phase 2 benchmark + freshness workload: fixed arrays and structs,
 * mutated across stops (element writes, field writes, pointer field). */
#include <stdlib.h>

struct point {
    int x;
    int y;
};

struct path {
    struct point waypoints[4];
    struct point *cursor;
    int len;
};

static struct path g_path;

int main(void) {
    struct point origin = {0, 0};
    struct path local = {{{1, 2}, {3, 4}, {5, 6}, {7, 8}}, &origin, 4};
    g_path = local;
    int *xs = &local.waypoints[0].x;
    for (int i = 0; i < 4; i++) {
        local.waypoints[i].x = i * 10;
        local.waypoints[i].y = xs[i] + 1;
    }
    g_path.cursor = &local.waypoints[3];
    g_path.len = local.waypoints[3].x + local.waypoints[3].y;
    local.cursor->y = g_path.len - 1;
    return g_path.len == 77 && local.waypoints[0].y == 1 ? 0 : 1;
}
