/*
 * c-trace-engine allocation instrumentation runtime.
 *
 * Linked into the debug binary via `-Wl,--wrap=malloc|calloc|realloc|free`.
 * Every allocation event is appended, with unbuffered write(), to a
 * dedicated trace file named by the C_TRACE_EVENT_FILE environment variable
 * (never to the program's stdout/stderr).
 *
 * Event line format (space-separated, one event per line):
 *
 *     pid seq kind id addr size old_id old_addr caller status [nmemb element_size]
 *
 *   pid      OS pid of the recording process (distinguishes forked children)
 *   seq      per-process event sequence number (1-based, monotonic)
 *   kind     "malloc" | "calloc" | "realloc" | "free"
 *   id       allocation id (per-process counter; stable for the block's
 *            lifetime; 0 when there is no tracked allocation)
 *   addr     returned address, hexadecimal (0 on failure)
 *   size     requested size in bytes (calloc: nmemb*size)
 *   old_id   previous allocation id for realloc (0 otherwise)
 *   old_addr previous address for realloc (0 otherwise)
 *   caller   return address of the caller (resolved later via addr2line)
 *   status   ok | failed | ok_same | ok_moved | new | zero | null | unknown
 *   nmemb, element_size  original calloc operands (only on calloc lines);
 *                        size is 0 on product overflow, not a wrapped result
 *
 * Constraints honored by this runtime:
 *   - No dynamic allocation inside the tracking path: fixed-size table,
 *     stack buffers, snprintf + write() only.
 *   - A __thread recursion guard makes any allocation triggered *by the
 *     tracker itself* pass through unrecorded.
 *   - A single spinlock (no futexes, no allocation) serializes table and
 *     sequence updates; program semantics are unchanged: every wrapper calls
 *     the real allocator exactly once and returns its result verbatim.
 *   - Failed allocations never modify tracked state: a failed realloc keeps
 *     the original block alive and marked as such.
 *   - The table maps address -> {id,size}; it is keyed per-process and the
 *     Rust side additionally matches by address, so even a table overflow
 *     (tracked blocks beyond CTE_TABLE_SLOTS) stays reconstructable.
 *
 * Known limitations (documented in README):
 *   - --wrap only intercepts references from objects linked into the binary.
 *     Allocations made internally by shared libraries (glibc internals,
 *     dlsym-loaded allocators, strdup's internal allocation) are NOT wrapped.
 *   - __builtin_return_address(0) can be unreliable for tail-calls or fully
 *     inlined callers; -O0 keeps this rare.
 *   - If the environment variable is unset the runtime is fully passive.
 */

#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <limits.h>
#if ULONG_MAX != UINT64_MAX
#error "c-trace allocation runtime requires a 64-bit unsigned long target"
#endif
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <pthread.h>

/* 2^17 slots ≈ 131k concurrently tracked live addresses (3 MiB static). */
#define CTE_TABLE_SLOTS (1u << 17)
#define CTE_TABLE_MASK (CTE_TABLE_SLOTS - 1u)

typedef struct {
    unsigned long addr; /* 0 = empty slot */
    unsigned long id;
    unsigned long size;
} cte_slot;

static cte_slot g_table[CTE_TABLE_SLOTS];
static int g_fd = -1;
static int g_status_fd = -1;
static volatile int g_spin = 0;
static unsigned long g_seq; /* last issued event sequence */
static unsigned long g_ids; /* last issued allocation id */
static unsigned long g_write_failures;
static int g_child_reset;
static __thread int g_busy = 0;

#ifdef CTE_TESTING
/* Fixture-only: 1 partial, 2 EINTR, 3 EIO, 4 zero, 5 repeated EINTR,
 * 6 partial followed by EIO. These cannot be activated in production. */
static int g_test_event_fault;
static int g_test_status_fault;
void cte_test_fault_next_event(int mode)
{
    __atomic_store_n(&g_test_event_fault, mode, __ATOMIC_RELEASE);
}
void cte_test_fail_status_ack(void)
{
    __atomic_store_n(&g_test_status_fault, 1, __ATOMIC_RELEASE);
}
#endif

extern void *__real_malloc(size_t);
extern void *__real_calloc(size_t, size_t);
extern void *__real_realloc(void *, size_t);
extern void __real_free(void *);

/* ------------------------------------------------------------------ */
/* Minimal synchronization: test-and-set spinlock, no allocation.      */
/* ------------------------------------------------------------------ */

static void cte_lock(void)
{
    for (;;) {
        if (__sync_bool_compare_and_swap(&g_spin, 0, 1))
            break;
        while (g_spin) {
            /* spin */
        }
    }
    if (g_child_reset) {
        memset(g_table, 0, sizeof(g_table));
        g_child_reset = 0;
    }
}

static void cte_unlock(void)
{
    __sync_lock_release(&g_spin);
}

/* A child must not wait on a lock owned by a thread that no longer exists.
 * No prepare/parent lock is taken across third-party atfork handlers. This
 * callback is deliberately async-signal-safe and defers table clearing to
 * the child's first wrapper under its own lock. Fork from inside an allocator
 * or a foreign atfork handler that acquires the lock after ours is unsupported.
 * Child execution between fork and exec in a multithreaded process still obeys
 * POSIX's async-signal-safe restriction; the wrapper cannot make libc malloc
 * safe in an otherwise unsafe fork context. */
static void cte_after_fork_child(void)
{
    __atomic_store_n(&g_spin, 0, __ATOMIC_RELEASE);
    g_seq = 0;
    g_ids = 0;
    g_write_failures = 0;
    g_child_reset = 1;
}

#ifdef CTE_TESTING
void cte_test_hold_lock(int ready_fd, int release_fd)
{
    char byte = 'r';
    cte_lock();
    (void)write(ready_fd, &byte, 1);
    (void)read(release_fd, &byte, 1);
    cte_unlock();
}
#endif

/* ------------------------------------------------------------------ */
/* Fixed-size open-addressing table: address -> {id, size}.            */
/* Linear probing with standard back-shift deletion (no tombstones).   */
/* ------------------------------------------------------------------ */

static unsigned long cte_hash(unsigned long x)
{
    x ^= x >> 33;
    x *= 0xff51afd7ed558ccdUL;
    x ^= x >> 33;
    x *= 0xc4ceb9fe1a85ec53UL;
    x ^= x >> 33;
    return x;
}

static cte_slot *cte_find(unsigned long addr)
{
    unsigned long i = cte_hash(addr) & CTE_TABLE_MASK;
    for (unsigned long n = 0; n < CTE_TABLE_SLOTS; n++, i = (i + 1) & CTE_TABLE_MASK) {
        if (g_table[i].addr == 0)
            return NULL; /* hit an empty slot: not present */
        if (g_table[i].addr == addr)
            return &g_table[i];
    }
    return NULL;
}

static int cte_insert(unsigned long addr, unsigned long id, unsigned long size)
{
    unsigned long i = cte_hash(addr) & CTE_TABLE_MASK;
    for (unsigned long n = 0; n < CTE_TABLE_SLOTS; n++, i = (i + 1) & CTE_TABLE_MASK) {
        if (g_table[i].addr == 0) {
            g_table[i].addr = addr;
            g_table[i].id = id;
            g_table[i].size = size;
            return 1;
        }
    }
    return 0; /* table full: events still recorded, id resolution falls
                 back to the Rust-side address map */
}

static void cte_remove(unsigned long addr)
{
    unsigned long i = cte_hash(addr) & CTE_TABLE_MASK;
    for (unsigned long n = 0; n < CTE_TABLE_SLOTS; n++, i = (i + 1) & CTE_TABLE_MASK) {
        if (g_table[i].addr == 0)
            return;
        if (g_table[i].addr == addr) {
            g_table[i].addr = 0;
            /* Re-insert every following non-empty slot to keep probe
             * chains intact (classic linear-probe deletion). */
            unsigned long k = (i + 1) & CTE_TABLE_MASK;
            while (g_table[k].addr != 0) {
                cte_slot moved = g_table[k];
                g_table[k].addr = 0;
                cte_insert(moved.addr, moved.id, moved.size);
                k = (k + 1) & CTE_TABLE_MASK;
            }
            return;
        }
    }
}

/* ------------------------------------------------------------------ */
/* Event emission: bounded retries into independent append-only files. */
/* ------------------------------------------------------------------ */

static int cte_write_all(int fd, const char *buf, size_t len, int event)
{
    size_t off = 0;
    unsigned int interruptions = 0;
#ifdef CTE_TESTING
    int fault = event ? __atomic_exchange_n(&g_test_event_fault, 0, __ATOMIC_ACQ_REL)
                      : __atomic_load_n(&g_test_status_fault, __ATOMIC_ACQUIRE) ? 3 : 0;
    int injected = 0;
#else
    (void)event;
#endif
    while (off < len) {
        ssize_t n;
#ifdef CTE_TESTING
        if (fault == 5 || (fault == 2 && !injected)) {
            n = -1;
            errno = EINTR;
            injected = 1;
        } else if (fault == 3 || (fault == 6 && injected)) {
            n = -1;
            errno = EIO;
        } else if (fault == 4) {
            n = 0;
        } else if ((fault == 1 || fault == 6) && !injected) {
            n = write(fd, buf + off, (len - off) / 2);
            injected = 1;
        } else
#endif
            n = write(fd, buf + off, len - off);
        if (n < 0) {
            if (errno == EINTR && ++interruptions <= 8)
                continue;
            return 0;
        }
        if (n == 0)
            return 0;
        off += (size_t)n;
    }
    return 1;
}

/* Caller must hold the lock. Bumps the sequence counter. */
static void cte_emit_locked(const char *kind, unsigned long id, unsigned long addr,
                            unsigned long size, unsigned long old_id, unsigned long old_addr,
                            unsigned long caller, const char *status,
                            unsigned long nmemb, unsigned long element_size)
{
    char buf[320];
    g_seq += 1;
    int n;
    if (kind[0] == 'c')
        n = snprintf(buf, sizeof(buf), "%lu %lu %s %lu %lx %lu %lu %lx %lx %s %lu %lu\n",
                     (unsigned long)getpid(), g_seq, kind, id, addr, size,
                     old_id, old_addr, caller, status, nmemb, element_size);
    else
        n = snprintf(buf, sizeof(buf), "%lu %lu %s %lu %lx %lu %lu %lx %lx %s\n",
                     (unsigned long)getpid(), g_seq, kind, id, addr, size,
                     old_id, old_addr, caller, status);
    if (n <= 0 || (size_t)n >= sizeof(buf) ||
        !cte_write_all(g_fd, buf, (size_t)n, 1)) {
        if (g_write_failures != ULONG_MAX)
            g_write_failures++;
    }
}

/* ------------------------------------------------------------------ */
/* Lifecycle                                                           */
/* ------------------------------------------------------------------ */

__attribute__((constructor(101)))
static void cte_init(void)
{
    const char *path = getenv("C_TRACE_EVENT_FILE");
    if (!path || !*path)
        return; /* passive mode: wrappers pass through, nothing recorded */
    g_fd = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
    if (g_fd < 0)
        return; /* no event writer means no valid terminal ACK */
    const char *status = getenv("C_TRACE_EVENT_STATUS_FILE");
    if (status && *status)
        g_status_fd = open(status, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
#if defined(CTE_TESTING) && defined(CTE_TEST_DISABLE_FORK_RESET)
    /* Regression-oracle build: retain the audited pre-repair behavior. */
#else
    if (pthread_atfork(NULL, NULL, cte_after_fork_child) != 0) {
        close(g_fd);
        g_fd = -1;
        if (g_status_fd >= 0) {
            close(g_status_fd);
            g_status_fd = -1;
        }
    }
#endif
}

__attribute__((destructor(101)))
static void cte_fini(void)
{
    if (g_fd >= 0 && g_status_fd >= 0) {
        /* Normal termination only; _exit/crash leaves no ACK. The status FD
         * is separate so event-write failures cannot suppress its report. */
        char buf[96];
        int n = snprintf(buf, sizeof(buf), "%lu %lu %lu\n",
                         (unsigned long)getpid(), g_seq, g_write_failures);
        if (n > 0 && (size_t)n < sizeof(buf))
            (void)cte_write_all(g_status_fd, buf, (size_t)n, 0);
        close(g_status_fd);
        g_status_fd = -1;
    }
    if (g_fd >= 0) {
        close(g_fd);
        g_fd = -1;
    }
}

/* ------------------------------------------------------------------ */
/* Wrapped allocation functions                                        */
/* ------------------------------------------------------------------ */

void *__wrap_malloc(size_t size)
{
    if (g_busy || g_fd < 0)
        return __real_malloc(size);
    g_busy = 1;
    unsigned long caller = (unsigned long)__builtin_return_address(0);
    cte_lock();
    void *ret = __real_malloc(size);
    int saved_errno = errno;
    unsigned long id = ++g_ids;
    cte_emit_locked("malloc", id, (unsigned long)ret, size, 0, 0, caller,
                    ret ? "ok" : "failed", 0, 0);
    if (ret)
        cte_insert((unsigned long)ret, id, size);
    cte_unlock();
    g_busy = 0;
    errno = saved_errno;
    return ret;
}

void *__wrap_calloc(size_t nmemb, size_t size)
{
    if (g_busy || g_fd < 0)
        return __real_calloc(nmemb, size);
    g_busy = 1;
    unsigned long caller = (unsigned long)__builtin_return_address(0);
    /* The wrapped product mirrors the ABI; the real calloc validates
     * overflow itself (returns NULL), which we record as "failed". */
    int overflow = size != 0 && nmemb > SIZE_MAX / size;
    unsigned long total = overflow ? 0 : nmemb * size;
    cte_lock();
    void *ret = __real_calloc(nmemb, size);
    int saved_errno = errno;
    unsigned long id = ++g_ids;
    cte_emit_locked("calloc", id, (unsigned long)ret, total, 0, 0, caller,
                    overflow ? "overflow" : (ret ? "ok" : "failed"), nmemb, size);
    if (ret)
        cte_insert((unsigned long)ret, id, total);
    cte_unlock();
    g_busy = 0;
    errno = saved_errno;
    return ret;
}

void *__wrap_realloc(void *ptr, size_t size)
{
    if (g_busy || g_fd < 0)
        return __real_realloc(ptr, size);
    g_busy = 1;
    unsigned long caller = (unsigned long)__builtin_return_address(0);

    cte_lock();
    unsigned long old_addr = (unsigned long)ptr;
    cte_slot *slot = old_addr ? cte_find(old_addr) : NULL;
    unsigned long old_id = slot ? slot->id : 0;
    /* Hold the bookkeeping lock while an old address may become reusable. */
    void *ret = __real_realloc(ptr, size);
    int saved_errno = errno;
    unsigned long id;
    const char *status;
    if (old_addr == 0) {
        id = ++g_ids;
        status = ret ? "new" : "failed"; /* realloc(NULL, n) may fail */
    } else if ((unsigned long)ret == old_addr) {
        id = ++g_ids; /* a successful realloc begins a new object lifetime */
        status = slot ? "ok_same" : "new"; /* untracked predecessor starts observation here */
    } else if (ret != NULL) {
        id = ++g_ids;
        status = "ok_moved"; /* old block retired, new id for new address */
    } else if (size == 0) {
        id = old_id;
        status = "zero"; /* glibc: realloc(p, 0) frees p and returns NULL */
    } else {
        id = old_id;
        status = "failed"; /* original allocation preserved */
    }
    cte_emit_locked("realloc", id, (unsigned long)ret, size, old_id,
                    old_addr, caller, status, 0, 0);

    /* Table maintenance (never before emission, and state changes must
     * match what the real allocator actually did). */
    if (old_addr != 0) {
        if ((unsigned long)ret == old_addr) {
            if (slot) {
                slot->id = id;
                slot->size = size;
            }
            else if (ret)
                cte_insert((unsigned long)ret, id, size);
        }
        else if (ret != NULL || size == 0)
            cte_remove(old_addr);
        /* status == "failed": keep the original entry */
    }
    if (ret != NULL && (old_addr == 0 || (unsigned long)ret != old_addr))
        cte_insert((unsigned long)ret, id, size);
    cte_unlock();
    g_busy = 0;
    errno = saved_errno;
    return ret;
}

void __wrap_free(void *ptr)
{
    if (g_busy || g_fd < 0) {
        __real_free(ptr);
        return;
    }
    g_busy = 1;
    unsigned long caller = (unsigned long)__builtin_return_address(0);
    cte_lock();
    unsigned long old_addr = (unsigned long)ptr;
    cte_slot *slot = old_addr ? cte_find(old_addr) : NULL;
    unsigned long id = slot ? slot->id : 0;
    const char *status = old_addr ? (slot ? "ok" : "unknown") : "null";
    unsigned long size = slot ? slot->size : 0;
    __real_free(ptr);
    int saved_errno = errno;
    cte_emit_locked("free", id, old_addr, size, 0, 0, caller, status, 0, 0);
    if (slot)
        cte_remove(old_addr);
    cte_unlock();
    g_busy = 0;
    errno = saved_errno;
}
