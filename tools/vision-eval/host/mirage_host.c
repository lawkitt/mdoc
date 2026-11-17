/* mirage_host: run Mirage through libcactus on a list of binary PPM (P6) images.
 *
 * Usage: mirage_host <mirage.cact> <options.json> <threads> <image.ppm>...
 *
 * Loads the archive once, then prints one JSON line per image to stdout:
 *   {"image": "<path>", "width": W, "height": H, "ms": T, "result": <describe JSON>}
 * or {"image": "<path>", "error": "<message>"}. Timing covers cactus_describe only.
 * Report-only evaluation tool (lawkitt/mdoc#911); nothing here ships.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "cactus.h"

#ifdef _WIN32
#include <windows.h>
static double now_ms(void) {
    LARGE_INTEGER f, c;
    QueryPerformanceFrequency(&f);
    QueryPerformanceCounter(&c);
    return (double)c.QuadPart * 1000.0 / (double)f.QuadPart;
}
#else
static double now_ms(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1000.0 + t.tv_nsec / 1e6;
}
#endif

static unsigned char *read_file(const char *path, long *size) {
    FILE *f = fopen(path, "rb");
    if (!f) return NULL;
    fseek(f, 0, SEEK_END);
    long n = ftell(f);
    fseek(f, 0, SEEK_SET);
    unsigned char *buf = malloc(n + 1);
    if (!buf || fread(buf, 1, n, f) != (size_t)n) { fclose(f); free(buf); return NULL; }
    buf[n] = 0;
    fclose(f);
    *size = n;
    return buf;
}

/* Skips whitespace and '#' comments in a PPM header. */
static const unsigned char *ppm_skip(const unsigned char *p, const unsigned char *end) {
    for (;;) {
        while (p < end && (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n')) p++;
        if (p < end && *p == '#') { while (p < end && *p != '\n') p++; continue; }
        return p;
    }
}

static const unsigned char *ppm_int(const unsigned char *p, const unsigned char *end, int *out) {
    p = ppm_skip(p, end);
    int v = 0, any = 0;
    while (p < end && *p >= '0' && *p <= '9') { v = v * 10 + (*p - '0'); p++; any = 1; }
    *out = any ? v : -1;
    return p;
}

static void json_string(const char *s) {
    putchar('"');
    for (; *s; s++) {
        unsigned char c = (unsigned char)*s;
        if (c == '"' || c == '\\') printf("\\%c", c);
        else if (c < 0x20) printf("\\u%04x", c);
        else putchar(c);
    }
    putchar('"');
}

int main(int argc, char **argv) {
    if (argc < 5) {
        fprintf(stderr, "usage: %s <mirage.cact> <options.json> <threads> <image.ppm>...\n", argv[0]);
        return 2;
    }
    long cact_size = 0, opt_size = 0;
    unsigned char *cact = read_file(argv[1], &cact_size);
    unsigned char *options = read_file(argv[2], &opt_size);
    if (!cact || !options) { fprintf(stderr, "cannot read %s or %s\n", argv[1], argv[2]); return 1; }
    int threads = atoi(argv[3]);
    if (threads > 0) cactus_set_threads(threads);
    double t0 = now_ms();
    if (cactus_load(cact, (unsigned long long)cact_size) < 0) {
        fprintf(stderr, "cactus_load: %s\n", cactus_last_error());
        return 1;
    }
    if (!(cactus_models() & CACTUS_VISION)) { fprintf(stderr, "not a vision archive\n"); return 1; }
    fprintf(stderr, "loaded in %.1f ms\n", now_ms() - t0);

    int cap = 32 * 1024 * 1024;
    char *out = malloc(cap);
    for (int i = 4; i < argc; i++) {
        long n = 0;
        unsigned char *img = read_file(argv[i], &n);
        const unsigned char *p = img, *end = img + n;
        int w = -1, h = -1, maxv = -1;
        if (img && n > 2 && p[0] == 'P' && p[1] == '6') {
            p = ppm_int(p + 2, end, &w);
            p = ppm_int(p, end, &h);
            p = ppm_int(p, end, &maxv);
            p++; /* the single whitespace byte after maxval */
        }
        printf("{\"image\":");
        json_string(argv[i]);
        if (w <= 0 || h <= 0 || maxv != 255 || end - p < (long)w * h * 3) {
            printf(",\"error\":\"not a binary 8-bit PPM\"}\n");
        } else {
            double s = now_ms();
            int code = cactus_describe(p, w, h, (const char *)options, NULL, NULL, out, cap);
            double ms = now_ms() - s;
            if (code < 0) {
                printf(",\"error\":");
                json_string(cactus_last_error());
                printf("}\n");
            } else {
                printf(",\"width\":%d,\"height\":%d,\"ms\":%.3f,\"result\":%s}\n", w, h, ms, out);
            }
        }
        fflush(stdout);
        free(img);
    }
    free(out);
    free(cact);
    free(options);
    return 0;
}
