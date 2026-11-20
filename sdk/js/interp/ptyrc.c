// SPDX-License-Identifier: GPL-3.0-or-later
// The script interpreter: QuickJS-ng as a guest of ABI 1
// (rust/ptyrc-box/README.md). It runs the ES modules handed to it in
// `init`; the module "ptyrc" is prelude.js, on the natives below.
//
// One thread, one context; the host calls one export at a time. The
// libc's WASI imports are defined here: the module imports `ptyrc`
// alone.

#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "quickjs.h"

#define IMPORT(name) __attribute__((import_module("ptyrc"), import_name(#name)))
IMPORT(call) int64_t h_call(const char *, size_t, const char *, size_t);
IMPORT(canvas) int32_t h_canvas(int32_t, int32_t);
IMPORT(cells) uint8_t *h_cells(int32_t);
IMPORT(present) int32_t h_present(int32_t, const char *, size_t);
IMPORT(sprite) int64_t h_sprite(int32_t);
IMPORT(release) void h_release(int32_t);
IMPORT(period) void h_period(int32_t);
IMPORT(log) void h_log(const char *, size_t);
IMPORT(random) void h_random(uint8_t *, size_t);
IMPORT(exit) _Noreturn void h_exit(int32_t);

static const char PRELUDE[] = {
#embed "prelude.js"
    , 0};
static const char DECK[] = {
#embed "deck.js"
    , 0};
static const char CANVAS[] = {
#embed "canvas.js"
    , 0};
static const char SIGHT[] = {
#embed "sight.js"
    , 0};

// Members of the `init` JSON, taken out before the script's `init`.
#define SOURCES_KEY "ptyrc:sources"
#define MAIN_KEY "ptyrc:main"
#define MANIFEST_KEY "ptyrc:manifest"
// Unit tests: the exports `test_*` of the main file, one an `event`.
#define UNIT_KEY "ptyrc:unit"
#define UNIT_TOPIC "ptyrc:unit"
// The API, and the natives under it: reachable until SEALED.
#define API "ptyrc"
#define NATIVE "ptyrc:native"
// The modules of the SDK beside the API, by name.
static const struct { const char *name, *src; size_t len; } MODULES[] = {
    {"ptyrc/deck", DECK, sizeof DECK - 1},
    {"ptyrc/canvas", CANVAS, sizeof CANVAS - 1},
    {"ptyrc/sight", SIGHT, sizeof SIGHT - 1},
};
#define NMODULES (sizeof MODULES / sizeof MODULES[0])

static int sdk_module(const char *name) {
    for (size_t i = 0; i < NMODULES; i++)
        if (!strcmp(name, MODULES[i].name)) return (int)i;
    return -1;
}

static JSRuntime *RT;
static JSContext *CTX;
static int64_t NOW_MS;   // session time of the last tick
static JSValue SOURCES;  // name -> text
static char *HOLDS;      // the names of SOURCES, for a hint
static JSValue MANIFEST = JS_NULL;  // JSON text, or null
static JSValue DESCRIBE = JS_UNDEFINED;  // prelude's: error -> text
static int SEALED;
static JSValue F_TICK = JS_UNDEFINED, F_EVENT = JS_UNDEFINED, F_STOP = JS_UNDEFINED;
static char *LAST;  // the last refusal logged
static int UNIT;    // unit mode
static JSValue TESTS = JS_UNDEFINED;  // [name, function] of `test_*`
static uint32_t NTESTS, NEXT, FAILED;

static void log_str(const char *s) { h_log(s, strlen(s)); }

// ---- WASI: the clock is session time; nothing else is reachable. ----

#define WASI(name) __imported_wasi_snapshot_preview1_##name
#define EBADF 8
#define ENOSYS 52
#define PTR(T, a) ((T *)(uintptr_t)(a))
int32_t WASI(clock_time_get)(int32_t id, int64_t prec, int32_t out) {
    *PTR(uint64_t, out) = (uint64_t)(NOW_MS < 0 ? 0 : NOW_MS) * 1000000u;
    return 0;
}
int32_t WASI(clock_res_get)(int32_t id, int32_t out) { *PTR(uint64_t, out) = 1000000; return 0; }
int32_t WASI(random_get)(int32_t buf, int32_t len) {
    for (int32_t i = 0; i < len; i += 65536)
        h_random(PTR(uint8_t, buf + i), len - i > 65536 ? 65536 : len - i);
    return 0;
}
// stdout, stderr: the log.
int32_t WASI(fd_write)(int32_t fd, int32_t iovs, int32_t n, int32_t out) {
    const uint32_t *v = PTR(const uint32_t, iovs);
    uint32_t total = 0;
    for (int32_t i = 0; i < n; i++) {
        if (v[2 * i + 1]) h_log(PTR(const char, v[2 * i]), v[2 * i + 1]);
        total += v[2 * i + 1];
    }
    *PTR(uint32_t, out) = total;
    return 0;
}
_Noreturn void WASI(proc_exit)(int32_t code) { h_exit(code); }
int32_t WASI(environ_sizes_get)(int32_t a, int32_t b) { *PTR(uint32_t, a) = *PTR(uint32_t, b) = 0; return 0; }
int32_t WASI(environ_get)(int32_t a, int32_t b) { return 0; }
int32_t WASI(args_sizes_get)(int32_t a, int32_t b) { *PTR(uint32_t, a) = *PTR(uint32_t, b) = 0; return 0; }
int32_t WASI(args_get)(int32_t a, int32_t b) { return 0; }
int32_t WASI(fd_close)(int32_t fd) { return EBADF; }
int32_t WASI(fd_seek)(int32_t fd, int64_t o, int32_t w, int32_t out) { return EBADF; }
int32_t WASI(fd_read)(int32_t fd, int32_t a, int32_t b, int32_t c) { return EBADF; }
int32_t WASI(fd_fdstat_get)(int32_t fd, int32_t out) { return EBADF; }
int32_t WASI(fd_fdstat_set_flags)(int32_t fd, int32_t f) { return EBADF; }
int32_t WASI(fd_prestat_get)(int32_t fd, int32_t out) { return EBADF; }
int32_t WASI(fd_prestat_dir_name)(int32_t fd, int32_t a, int32_t b) { return EBADF; }
int32_t WASI(fd_filestat_get)(int32_t fd, int32_t out) { return EBADF; }
int32_t WASI(path_open)(int32_t a, int32_t b, int32_t c, int32_t d, int32_t e, int64_t f, int64_t g, int32_t h,
                        int32_t i) { return ENOSYS; }
int32_t WASI(path_filestat_get)(int32_t a, int32_t b, int32_t c, int32_t d, int32_t e) { return ENOSYS; }
int32_t WASI(poll_oneoff)(int32_t a, int32_t b, int32_t c, int32_t d) { return ENOSYS; }
int32_t WASI(sched_yield)(void) { return 0; }

// ---- alloc, free: the host writes arguments into our memory. ----

__attribute__((export_name("alloc"))) void *g_alloc(size_t len) { return len ? calloc(1, len) : (void *)16; }
__attribute__((export_name("free"))) void g_free(void *p, size_t len) { if (len && p) free(p); }

// ---- canvases: their cells in our memory, as the host laid them. ----

typedef struct { int32_t id, w, h; uint8_t *ptr; } Cv;
static Cv *CVS;
static size_t NCV, CAPCV;

static Cv *cv_of(JSContext *ctx, int argc, JSValueConst *argv) {
    int32_t id = 0;
    if (argc > 0) JS_ToInt32(ctx, &id, argv[0]);
    for (size_t i = 0; id && i < NCV; i++)
        if (CVS[i].id == id) return &CVS[i];
    return NULL;
}

// Columns of a scalar value, coarsely; the runner has the last word.
// 0: joins the glyph before (dropped by `text`); 2: wide; else 1.
static int width_of(uint32_t c) {
    static const uint32_t ZERO[][2] = {
        {0x0300, 0x036f}, {0x200b, 0x200f}, {0x202a, 0x202e}, {0x2060, 0x2064},
        {0xfe00, 0xfe0f}, {0xfeff, 0xfeff}, {0xe0000, 0xe0fff},
    };
    static const uint32_t WIDE[][2] = {
        {0x1100, 0x115f}, {0x2e80, 0x303e}, {0x3041, 0x33ff},   {0x3400, 0x4dbf},   {0x4e00, 0x9fff},
        {0xa000, 0xa4cf}, {0xac00, 0xd7a3}, {0xf900, 0xfaff},   {0xfe30, 0xfe4f},   {0xff00, 0xff60},
        {0xffe0, 0xffe6}, {0x1f300, 0x1f64f}, {0x1f900, 0x1f9ff}, {0x20000, 0x3fffd},
    };
    if (c < 0x300) return 1;
    for (size_t i = 0; i < sizeof ZERO / sizeof *ZERO; i++)
        if (c >= ZERO[i][0] && c <= ZERO[i][1]) return 0;
    for (size_t i = 0; i < sizeof WIDE / sizeof *WIDE; i++)
        if (c >= WIDE[i][0] && c <= WIDE[i][1]) return 2;
    return 1;
}

// One cell of 16 bytes: scalar (LE), fg, bg (BE), attributes (LE).
static void put(Cv *cv, int32_t x, int32_t y, uint32_t s, uint32_t fg, uint32_t bg, uint32_t attrs) {
    if (x < 0 || y < 0 || x >= cv->w || y >= cv->h) return;
    uint8_t *p = cv->ptr + ((size_t)y * cv->w + x) * 16;
    memcpy(p, &s, 4);
    p[4] = fg >> 24, p[5] = fg >> 16, p[6] = fg >> 8, p[7] = fg;
    p[8] = bg >> 24, p[9] = bg >> 16, p[10] = bg >> 8, p[11] = bg;
    p[12] = attrs, p[13] = attrs >> 8, p[14] = 0, p[15] = 0;
}

// ---- the natives: numbers in, numbers and texts out ----

#define NATIVE_FN(name) static JSValue n_##name(JSContext *ctx, JSValueConst this, int argc, JSValueConst *argv)

static int32_t I(JSContext *ctx, int argc, JSValueConst *argv, int i) {
    int32_t v = 0;
    if (i < argc) JS_ToInt32(ctx, &v, argv[i]);
    return v;
}
static uint32_t U(JSContext *ctx, int argc, JSValueConst *argv, int i) {
    uint32_t v = 0;
    if (i < argc) JS_ToUint32(ctx, &v, argv[i]);
    return v;
}
static JSValueConst A(int argc, JSValueConst *argv, int i) { return i < argc ? argv[i] : JS_UNDEFINED; }

NATIVE_FN(call) {
    size_t ml, pl;
    const char *m = JS_ToCStringLen(ctx, &ml, A(argc, argv, 0));
    const char *p = m ? JS_ToCStringLen(ctx, &pl, A(argc, argv, 1)) : NULL;
    if (!p) { JS_FreeCString(ctx, m); return JS_EXCEPTION; }
    int64_t packed = h_call(m, ml, p, pl);
    JS_FreeCString(ctx, m);
    JS_FreeCString(ctx, p);
    if (!packed) return JS_ThrowRangeError(ctx, "call: no memory left for the answer");
    char *ptr = PTR(char, (uint64_t)packed >> 32);
    size_t len = (uint64_t)packed & 0xffffffffu;
    JSValue s = JS_NewStringLen(ctx, ptr, len);
    g_free(ptr, len);
    return s;
}
NATIVE_FN(canvas) {
    int32_t w = I(ctx, argc, argv, 0), h = I(ctx, argc, argv, 1);
    if (w <= 0 || h <= 0) return JS_NewInt32(ctx, 0);
    if (NCV == CAPCV) {
        size_t cap = CAPCV ? 2 * CAPCV : 16;
        Cv *grown = realloc(CVS, cap * sizeof *CVS);
        if (!grown) return JS_NewInt32(ctx, 0);
        CVS = grown, CAPCV = cap;
    }
    int32_t id = h_canvas(w, h);
    uint8_t *ptr = id ? h_cells(id) : NULL;
    if (!ptr) return JS_NewInt32(ctx, 0);
    CVS[NCV++] = (Cv){id, w, h, ptr};
    return JS_NewInt32(ctx, id);
}
// set(id, x, y, scalar, fg, bg, attrs)
NATIVE_FN(set) {
    Cv *cv = cv_of(ctx, argc, argv);
    if (cv)
        put(cv, I(ctx, argc, argv, 1), I(ctx, argc, argv, 2), U(ctx, argc, argv, 3), U(ctx, argc, argv, 4),
            U(ctx, argc, argv, 5), U(ctx, argc, argv, 6));
    return JS_UNDEFINED;
}
// fill(id, x, y, w, h, scalar, fg, bg, attrs)
NATIVE_FN(fill) {
    Cv *cv = cv_of(ctx, argc, argv);
    if (!cv) return JS_UNDEFINED;
    int64_t x = I(ctx, argc, argv, 1), y = I(ctx, argc, argv, 2);
    int64_t xe = x + I(ctx, argc, argv, 3), ye = y + I(ctx, argc, argv, 4);
    uint32_t s = U(ctx, argc, argv, 5), fg = U(ctx, argc, argv, 6), bg = U(ctx, argc, argv, 7);
    uint32_t at = U(ctx, argc, argv, 8);
    int step = width_of(s) == 2 ? 2 : 1;
    for (int64_t yy = y < 0 ? 0 : y; yy < ye && yy < cv->h; yy++)
        for (int64_t xx = x; xx < xe && xx < cv->w; xx += step) put(cv, xx, yy, s, fg, bg, at);
    return JS_UNDEFINED;
}
// text(id, x, y, string, fg, bg, attrs) -> x after the last glyph
NATIVE_FN(text) {
    int32_t x = I(ctx, argc, argv, 1);
    Cv *cv = cv_of(ctx, argc, argv);
    if (!cv) return JS_NewInt32(ctx, x);
    int32_t y = I(ctx, argc, argv, 2);
    uint32_t fg = U(ctx, argc, argv, 4), bg = U(ctx, argc, argv, 5), at = U(ctx, argc, argv, 6);
    size_t len;
    const char *s = JS_ToCStringLen(ctx, &len, A(argc, argv, 3));
    if (!s) return JS_EXCEPTION;
    for (const unsigned char *p = (const unsigned char *)s, *e = p + len; p < e;) {
        uint32_t c = *p++;  // UTF-8 (WTF-8 for a lone surrogate)
        int more = c >= 0xf0 ? 3 : c >= 0xe0 ? 2 : c >= 0xc0 ? 1 : 0;
        c &= 0x7f >> more;
        while (more-- && p < e) c = c << 6 | (*p++ & 63);
        int w = width_of(c);
        if (!w) continue;
        put(cv, x, y, c, fg, bg, at);
        x = x > INT32_MAX - w ? INT32_MAX : x + w;
    }
    JS_FreeCString(ctx, s);
    return JS_NewInt32(ctx, x);
}
// cells(string) -> columns
NATIVE_FN(cells) {
    size_t len;
    const char *s = JS_ToCStringLen(ctx, &len, A(argc, argv, 0));
    if (!s) return JS_EXCEPTION;
    int64_t n = 0;
    for (const unsigned char *p = (const unsigned char *)s, *e = p + len; p < e;) {
        uint32_t c = *p++;
        int more = c >= 0xf0 ? 3 : c >= 0xe0 ? 2 : c >= 0xc0 ? 1 : 0;
        c &= 0x7f >> more;
        while (more-- && p < e) c = c << 6 | (*p++ & 63);
        n += width_of(c);
    }
    JS_FreeCString(ctx, s);
    return JS_NewInt64(ctx, n);
}
// bytes(id) -> Uint8Array: a copy of the cells
NATIVE_FN(bytes) {
    Cv *cv = cv_of(ctx, argc, argv);
    if (!cv) return JS_NewUint8ArrayCopy(ctx, NULL, 0);
    return JS_NewUint8ArrayCopy(ctx, cv->ptr, (size_t)cv->w * cv->h * 16);
}
NATIVE_FN(clear) {
    Cv *cv = cv_of(ctx, argc, argv);
    if (cv) memset(cv->ptr, 0, (size_t)cv->w * cv->h * 16);
    return JS_UNDEFINED;
}
// present(id, place) -> boolean; place: JSON
NATIVE_FN(present) {
    Cv *cv = cv_of(ctx, argc, argv);
    if (!cv) return JS_FALSE;
    size_t len;
    const char *s = JS_ToCStringLen(ctx, &len, A(argc, argv, 1));
    if (!s) return JS_EXCEPTION;
    bool shown = h_present(cv->id, s, len) == 0;
    JS_FreeCString(ctx, s);
    return JS_NewBool(ctx, shown);
}
// sprite(id) -> the id of its sprite, or null
NATIVE_FN(sprite) {
    Cv *cv = cv_of(ctx, argc, argv);
    int64_t id = cv ? h_sprite(cv->id) : -1;
    return id < 0 ? JS_NULL : JS_NewInt64(ctx, id);
}
NATIVE_FN(release) {
    Cv *cv = cv_of(ctx, argc, argv);
    if (cv) h_release(cv->id), *cv = CVS[--NCV];
    return JS_UNDEFINED;
}
NATIVE_FN(period) {
    int32_t ms = I(ctx, argc, argv, 0);
    h_period(ms < 0 ? 0 : ms);
    return JS_UNDEFINED;
}
NATIVE_FN(log) {
    size_t len;
    const char *s = JS_ToCStringLen(ctx, &len, A(argc, argv, 0));
    if (!s) return JS_EXCEPTION;
    h_log(s, len);
    JS_FreeCString(ctx, s);
    return JS_UNDEFINED;
}
NATIVE_FN(random) {
    int32_t n = I(ctx, argc, argv, 0);
    n = n < 0 ? 0 : n > 65536 ? 65536 : n;
    uint8_t *b = malloc(n ? n : 1);
    if (!b) return JS_ThrowOutOfMemory(ctx);
    if (n) h_random(b, n);
    JSValue a = JS_NewUint8ArrayCopy(ctx, b, n);
    free(b);
    return a;
}
// Math.random: 53 bits of the host's random, drawn 4 KiB at a time.
NATIVE_FN(math_random) {
    static uint64_t pool[512];
    static size_t left;
    if (!left) h_random((uint8_t *)pool, sizeof pool), left = 512;
    return JS_NewFloat64(ctx, (double)(pool[--left] >> 11) * 0x1p-53);
}
NATIVE_FN(exit) { h_exit(I(ctx, argc, argv, 0)); }
NATIVE_FN(manifest) { return JS_DupValue(ctx, MANIFEST); }
// hooks(describe): the prelude's error -> text, taken once.
NATIVE_FN(hooks) {
    if (JS_IsUndefined(DESCRIBE) && argc > 0 && JS_IsFunction(ctx, argv[0])) DESCRIBE = JS_DupValue(ctx, argv[0]);
    return JS_UNDEFINED;
}

static const JSCFunctionListEntry NATIVES[] = {
    JS_CFUNC_DEF("call", 2, n_call),       JS_CFUNC_DEF("canvas", 2, n_canvas),
    JS_CFUNC_DEF("set", 7, n_set),         JS_CFUNC_DEF("fill", 9, n_fill),
    JS_CFUNC_DEF("text", 7, n_text),       JS_CFUNC_DEF("clear", 1, n_clear),
    JS_CFUNC_DEF("present", 2, n_present), JS_CFUNC_DEF("release", 1, n_release),
    JS_CFUNC_DEF("period", 1, n_period),   JS_CFUNC_DEF("log", 1, n_log),
    JS_CFUNC_DEF("random", 1, n_random),   JS_CFUNC_DEF("mathRandom", 0, n_math_random),
    JS_CFUNC_DEF("exit", 1, n_exit),       JS_CFUNC_DEF("manifest", 0, n_manifest),
    JS_CFUNC_DEF("hooks", 1, n_hooks),     JS_CFUNC_DEF("cells", 1, n_cells),
    JS_CFUNC_DEF("bytes", 1, n_bytes),     JS_CFUNC_DEF("sprite", 1, n_sprite),
};
#define NNATIVES (int)(sizeof NATIVES / sizeof *NATIVES)
static int natives_init(JSContext *ctx, JSModuleDef *m) { return JS_SetModuleExportList(ctx, m, NATIVES, NNATIVES); }

// ---- modules: "ptyrc", and the files of the tree by relative path ----

// A plain Error, its message of any length (JS_Throw* cut at 256
// bytes). Returns NULL.
static void *throw_fmt(JSContext *ctx, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(NULL, 0, fmt, ap);
    va_end(ap);
    char *s = n < 0 ? NULL : malloc(n + 1);
    if (!s) return JS_ThrowOutOfMemory(ctx), NULL;
    va_start(ap, fmt);
    vsnprintf(s, n + 1, fmt, ap);
    va_end(ap);
    JSValue e = JS_NewError(ctx);
    JS_DefinePropertyValueStr(ctx, e, "message", JS_NewString(ctx, s), JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
    free(s);
    JS_Throw(ctx, e);
    return NULL;
}

// `path` under `dir` (the part of a name before its last '/'), without
// `.` and `..`. NULL: it leaves the tree, is absolute, or is empty.
static char *normal(JSContext *ctx, const char *dir, size_t dirlen, const char *path) {
    if (path[0] == '/' || strchr(path, '\\')) return NULL;
    char *out = js_malloc(ctx, dirlen + strlen(path) + 2);
    if (!out) return NULL;
    size_t n = 0;
    memcpy(out, dir, dirlen), n = dirlen;
    for (const char *p = path; *p;) {
        const char *e = strchr(p, '/');
        size_t len = e ? (size_t)(e - p) : strlen(p);
        if (len == 2 && p[0] == '.' && p[1] == '.') {
            if (!n) { js_free(ctx, out); return NULL; }
            while (n && out[n - 1] != '/') n--;
            if (n) n--;
        } else if (len && !(len == 1 && p[0] == '.')) {
            if (n) out[n++] = '/';
            memcpy(out + n, p, len), n += len;
        }
        p += len + (e != NULL);
    }
    out[n] = 0;
    if (!n) { js_free(ctx, out); return NULL; }
    return out;
}

static char *normalize(JSContext *ctx, const char *base, const char *spec, void *opaque) {
    if (!strcmp(spec, API) || sdk_module(spec) >= 0 || (!strcmp(spec, NATIVE) && !SEALED))
        return js_strdup(ctx, spec);
    if (strncmp(spec, "./", 2) && strncmp(spec, "../", 3)) {
        return throw_fmt(ctx, "import \"%s\": no such module\n  hint: \"ptyrc\", \"ptyrc/deck\", \"ptyrc/canvas\", \"ptyrc/sight\", or a file of the app by relative path: \"./%s\"",
                         spec, spec + strspn(spec, "/"));
    }
    const char *slash = strrchr(base, '/');
    char *name = normal(ctx, base, slash ? (size_t)(slash - base) : 0, spec);
    if (!name) {
        if (JS_HasException(ctx)) return NULL;
        return throw_fmt(ctx, "import \"%s\" from %s: outside the app\n  hint: imports stay inside the tree of the app",
                         spec, base);
    }
    if (!strcmp(name, NATIVE) || sdk_module(name) >= 0) {  // a file of that name: none is
        throw_fmt(ctx, "`%s`: no such file in the app", name);
        js_free(ctx, name);
        return NULL;
    }
    return name;
}

static JSModuleDef *compile(JSContext *ctx, const char *name, const char *src, size_t len) {
    JSValue f = JS_Eval(ctx, src, len, name, JS_EVAL_TYPE_MODULE | JS_EVAL_FLAG_COMPILE_ONLY);
    if (JS_IsException(f)) return NULL;
    JSModuleDef *m = JS_VALUE_GET_PTR(f);
    JS_FreeValue(ctx, f);
    return m;
}

static JSModuleDef *load(JSContext *ctx, const char *name, void *opaque) {
    if (!strcmp(name, NATIVE)) {
        JSModuleDef *m = JS_NewCModule(ctx, name, natives_init);
        if (m) JS_AddModuleExportList(ctx, m, NATIVES, NNATIVES);
        return m;
    }
    if (!strcmp(name, API)) return compile(ctx, name, PRELUDE, sizeof PRELUDE - 1);
    int sdk = sdk_module(name);
    if (sdk >= 0) return compile(ctx, name, MODULES[sdk].src, MODULES[sdk].len);
    JSValue text = JS_GetPropertyStr(ctx, SOURCES, name);
    if (!JS_IsString(text)) {
        JS_FreeValue(ctx, text);
        throw_fmt(ctx, "`%s`: no such file in the app\n  hint: the app holds: %s", name, HOLDS);
        return NULL;
    }
    size_t len;
    const char *src = JS_ToCStringLen(ctx, &len, text);
    JS_FreeValue(ctx, text);
    if (!src) return NULL;
    JSModuleDef *m = compile(ctx, name, src, len);
    JS_FreeCString(ctx, src);
    return m;
}

// The names of SOURCES, ", " between; "nothing" when none.
static char *holds(void) {
    JSPropertyEnum *tab;
    uint32_t n;
    size_t size = 8, at = 0;
    char *out = malloc(size);
    if (!out || JS_GetOwnPropertyNames(CTX, &tab, &n, SOURCES, JS_GPN_STRING_MASK | JS_GPN_ENUM_ONLY) < 0)
        return strcpy(out ? out : malloc(8), "nothing");
    for (uint32_t i = 0; i < n; i++) {
        const char *s = JS_AtomToCString(CTX, tab[i].atom);
        size_t len = s ? strlen(s) : 0;
        if ((out = realloc(out, size += len + 2)) && s) {
            if (at) memcpy(out + at, ", ", 2), at += 2;
            memcpy(out + at, s, len), at += len;
        }
        JS_FreeCString(CTX, s);
    }
    JS_FreePropertyEnum(CTX, tab, n);
    if (!at) strcpy(out, "nothing");
    else out[at] = 0;
    return out;
}

// ---- errors ----

// The exception in flight as lines of the log (the prelude's
// `describe`): `file:line:column: what`, the frames of the app, the
// cure when known. Logged; then the app ends, unless it is a
// `PtyrcError` out of `tick`, `event`, `stop` (`soft`): that is told
// once while it repeats, and the app goes on.
static void report(int soft) {
    JSValue e = JS_GetException(CTX);
    JSValue text = JS_UNDEFINED;
    if (JS_IsFunction(CTX, DESCRIBE)) {
        text = JS_Call(CTX, DESCRIBE, JS_UNDEFINED, 1, &e);
        if (JS_IsException(text)) JS_FreeValue(CTX, JS_GetException(CTX)), text = JS_UNDEFINED;
    }
    // Not described: no memory left, as a rule.
    const char *line = JS_IsString(text) || JS_IsError(e) ? JS_ToCString(CTX, JS_IsString(text) ? text : e) : NULL;
    if (!line) JS_FreeValue(CTX, JS_GetException(CTX));
    const char *shown = line ? line : "an error, and no memory left to tell it";
    if (soft && JS_IsError(e)) {
        JSValue name = JS_GetPropertyStr(CTX, e, "name");
        const char *n = JS_IsString(name) ? JS_ToCString(CTX, name) : NULL;
        soft = n && !strcmp(n, "PtyrcError");
        JS_FreeCString(CTX, n);
        JS_FreeValue(CTX, name);
    } else {
        soft = 0;
    }
    if (!soft) log_str(shown), h_exit(1);
    if (!LAST || strcmp(LAST, shown)) {
        log_str(shown);
        free(LAST);
        LAST = strdup(shown);
    }
    JS_FreeCString(CTX, line);
    JS_FreeValue(CTX, text);
    JS_FreeValue(CTX, e);
}

// The jobs queued run to the end (a chain without end: the runner's
// time limit). -1: one threw.
static int run_jobs(void) {
    JSContext *c;
    int r;
    while ((r = JS_ExecutePendingJob(RT, &c)) > 0) {}
    return r < 0 ? -1 : 0;
}

// A promise taken: settled, else its error thrown. -1: thrown.
static int settled(JSValue promise, const char *pending) {
    int st = JS_PromiseState(CTX, promise);
    if (st == JS_PROMISE_REJECTED) {
        JS_Throw(CTX, JS_PromiseResult(CTX, promise));
    } else if (st == JS_PROMISE_PENDING && pending) {
        JS_ThrowPlainError(CTX, "%s", pending);
    }
    JS_FreeValue(CTX, promise);
    return JS_HasException(CTX) ? -1 : 0;
}

// Load, link, evaluate a module; its jobs run. -1: thrown.
static int settle(JSModuleDef *m) {
    if (!m) return -1;
    JSValue v = JS_DupValue(CTX, JS_MKPTR(JS_TAG_MODULE, m));
    if (JS_ResolveModule(CTX, v) < 0) return JS_FreeValue(CTX, v), -1;
    JSValue promise = JS_EvalFunction(CTX, v);
    if (JS_IsException(promise) || run_jobs() < 0) return JS_FreeValue(CTX, promise), -1;
    if (!JS_IsPromise(promise)) return JS_FreeValue(CTX, promise), 0;
    return settled(promise, "a module awaits what never comes: no promise outlives a call");
}

// Call an export; the jobs it queued run before it returns. A promise
// it returns (an `async` export) and that is rejected is its error.
static int run(JSValue f, int argc, JSValue *argv) {
    JSValue out = JS_Call(CTX, f, JS_UNDEFINED, argc, argv);
    if (JS_IsException(out)) return -1;
    if (run_jobs() < 0) return JS_FreeValue(CTX, out), -1;
    if (!JS_IsPromise(out)) return JS_FreeValue(CTX, out), 0;
    return settled(out, NULL);
}

// ---- the exports of ABI 1 ----

static JSValue parse(const char *p, size_t n, const char *what) {
    char *z = malloc(n + 1);
    if (!z) return JS_ThrowOutOfMemory(CTX);
    memcpy(z, p, n), z[n] = 0;
    JSValue v = JS_ParseJSON(CTX, z, n, what);
    free(z);
    return v;
}

// A member of `o` taken out.
static JSValue take(JSValue o, const char *key) {
    JSValue v = JS_GetPropertyStr(CTX, o, key);
    JSAtom a = JS_NewAtom(CTX, key);
    JS_DeleteProperty(CTX, o, a, 0);
    JS_FreeAtom(CTX, a);
    return v;
}

// A line of the log, then the end.
static _Noreturn void die(const char *fmt, ...) {
    char line[1024];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(line, sizeof line, fmt, ap);
    va_end(ap);
    log_str(line);
    h_exit(1);
}

// An export of the script; undefined if absent. Not a function: ends.
static JSValue export_fn(JSValue ns, const char *main, const char *name) {
    JSValue v = JS_GetPropertyStr(CTX, ns, name);
    if (JS_IsUndefined(v) || JS_IsFunction(CTX, v)) return v;
    die("%s: the export `%s` is not a function\n  hint: export function %s(...) { }", main, name, name);
}

// ---- unit tests ----

// The exports `test_*` of `ns`, in the order of their names, into
// TESTS; their names on one line of the log, for the runner.
static void unit_init(JSValue ns, const char *main) {
    JSPropertyEnum *tab;
    uint32_t n;
    if (JS_GetOwnPropertyNames(CTX, &tab, &n, ns, JS_GPN_STRING_MASK | JS_GPN_ENUM_ONLY) < 0) report(0);
    TESTS = JS_NewArray(CTX);
    size_t size = 32, at = 0;
    char *line = malloc(size);
    if (!line) die("interpreter: no memory");
    at = (size_t)snprintf(line, size, "ptyrc:tests");
    for (uint32_t i = 0; i < n; i++) {
        const char *name = JS_AtomToCString(CTX, tab[i].atom);
        if (name && !strncmp(name, "test_", 5)) {
            JSValue f = JS_GetProperty(CTX, ns, tab[i].atom);
            if (JS_IsFunction(CTX, f)) {
                JS_SetPropertyUint32(CTX, TESTS, NTESTS++, f);
                size_t len = strlen(name);
                if (at + len + 2 > size && !(line = realloc(line, size = 2 * (at + len + 2)))) die("interpreter: no memory");
                line[at++] = ' ';
                memcpy(line + at, name, len + 1), at += len;
            } else {
                JS_FreeValue(CTX, f);
            }
        }
        JS_FreeCString(CTX, name);
    }
    JS_FreePropertyEnum(CTX, tab, n);
    if (!NTESTS) die("%s: no export test_*\n  hint: export function test_sum() { assertEqual(1 + 1, 2); }", main);
    log_str(line);
    free(line);
}

// The next test: `ok NAME`, or `FAIL NAME` and its error; after the
// last, the count, and the end: status 1 when one failed.
static void unit_next(void) {
    if (NEXT >= NTESTS) {
        char line[96];
        snprintf(line, sizeof line, "%u passed, %u failed", NTESTS - FAILED, FAILED);
        log_str(line);
        h_exit(FAILED ? 1 : 0);
    }
    JSValue f = JS_GetPropertyUint32(CTX, TESTS, NEXT++);
    JSValue nv = JS_GetPropertyStr(CTX, f, "name");
    const char *name = JS_ToCString(CTX, nv);
    char head[160];
    if (run(f, 0, NULL) == 0) {
        snprintf(head, sizeof head, "ok   %s", name ? name : "?");
        log_str(head);
    } else {
        FAILED++;
        snprintf(head, sizeof head, "FAIL %s", name ? name : "?");
        log_str(head);
        JSValue e = JS_GetException(CTX);
        JSValue text = JS_IsFunction(CTX, DESCRIBE) ? JS_Call(CTX, DESCRIBE, JS_UNDEFINED, 1, &e) : JS_UNDEFINED;
        if (JS_IsException(text)) JS_FreeValue(CTX, JS_GetException(CTX)), text = JS_UNDEFINED;
        const char *t = JS_ToCString(CTX, JS_IsString(text) ? text : e);
        if (!t) JS_FreeValue(CTX, JS_GetException(CTX));
        log_str(t ? t : "an error, and no memory left to tell it");
        JS_FreeCString(CTX, t);
        JS_FreeValue(CTX, text);
        JS_FreeValue(CTX, e);
    }
    JS_FreeCString(CTX, name);
    JS_FreeValue(CTX, nv);
    JS_FreeValue(CTX, f);
}

extern void __wasm_call_ctors(void);

__attribute__((export_name("init"))) void g_init(const char *ptr, size_t len) {
    __wasm_call_ctors();
    RT = JS_NewRuntime();
    // Of the 1 MiB stack in memory: past 352 KiB, a RangeError. More
    // and the runner's stack (1 MiB) runs out first: at about 430 KiB
    // for a getter that calls itself, the most runner's stack a byte.
    JS_SetMaxStackSize(RT, 352 * 1024);
    CTX = JS_NewContext(RT);
    if (!CTX) die("interpreter: no memory");
    JS_SetModuleLoaderFunc(RT, normalize, load, NULL);

    JSValue args = len ? parse(ptr, len, "init") : JS_NewObject(CTX);
    if (JS_IsException(args)) report(0);
    if (!JS_IsObject(args) || JS_IsArray(args)) JS_FreeValue(CTX, args), args = JS_NewObject(CTX);
    SOURCES = take(args, SOURCES_KEY);
    if (!JS_IsObject(SOURCES)) JS_FreeValue(CTX, SOURCES), SOURCES = JS_NewObject(CTX);
    HOLDS = holds();
    JSValue mainv = take(args, MAIN_KEY);
    JSValue m = take(args, MANIFEST_KEY);
    if (!JS_IsUndefined(m)) MANIFEST = JS_JSONStringify(CTX, m, JS_UNDEFINED, JS_UNDEFINED);
    if (JS_IsException(MANIFEST)) report(0);
    JS_FreeValue(CTX, m);
    JSValue unit = take(args, UNIT_KEY);
    UNIT = JS_ToBool(CTX, unit) > 0;
    JS_FreeValue(CTX, unit);

    // The API first, while the natives are reachable; then never again.
    if (settle(load(CTX, API, NULL)) < 0) report(0);
    SEALED = 1;

    const char *given = JS_IsString(mainv) ? JS_ToCString(CTX, mainv) : NULL;
    const char *main = normal(CTX, "", 0, given ? given : "app.js");
    if (!main) die("`%s`: not a path inside the app", given);
    JSModuleDef *entry = load(CTX, main, NULL);
    if (settle(entry) < 0) report(0);
    JSValue ns = JS_GetModuleNamespace(CTX, entry);
    if (UNIT) {
        unit_init(ns, main);
        JS_FreeValue(CTX, ns);
        JS_FreeValue(CTX, args);
        JS_FreeValue(CTX, mainv);
        return;
    }
    JSValue init = export_fn(ns, main, "init");
    F_TICK = export_fn(ns, main, "tick");
    F_EVENT = export_fn(ns, main, "event");
    F_STOP = export_fn(ns, main, "stop");
    JS_FreeValue(CTX, ns);
    if (JS_IsUndefined(init) && JS_IsUndefined(F_TICK) && JS_IsUndefined(F_EVENT))
        die("%s: exports none of `init`, `tick`, `event`\n  hint: export function init(args) { period(50); } "
            "and export function tick(now) { }", main);
    if (!JS_IsUndefined(init) && run(init, 1, &args) < 0) report(0);
    JS_FreeValue(CTX, init);
    JS_FreeValue(CTX, args);
    JS_FreeValue(CTX, mainv);
}

__attribute__((export_name("tick"))) void g_tick(int64_t now) {
    NOW_MS = now;
    if (JS_IsUndefined(F_TICK)) return;
    JSValue a = JS_NewFloat64(CTX, (double)now);
    if (run(F_TICK, 1, &a) < 0) report(1);
}

__attribute__((export_name("event"))) void g_event(const char *tp, size_t tl, const char *dp, size_t dl) {
    if (UNIT) {
        if (tl == sizeof UNIT_TOPIC - 1 && !memcmp(tp, UNIT_TOPIC, tl)) unit_next();
        return;
    }
    if (JS_IsUndefined(F_EVENT)) return;
    JSValue a[2] = {JS_NewStringLen(CTX, tp, tl), dl ? parse(dp, dl, "event") : JS_NULL};
    if (JS_IsException(a[1])) JS_FreeValue(CTX, JS_GetException(CTX)), a[1] = JS_NULL;
    int r = run(F_EVENT, 2, a);
    JS_FreeValue(CTX, a[0]);
    JS_FreeValue(CTX, a[1]);
    if (r < 0) report(1);
}

__attribute__((export_name("stop"))) void g_stop(void) {
    if (!JS_IsUndefined(F_STOP) && run(F_STOP, 0, NULL) < 0) report(1);
}
