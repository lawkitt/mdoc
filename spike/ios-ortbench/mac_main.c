// SPIKE (#903): macOS CLI around bench.c. usage: ortbench-mac <model> <cpu|coreml> <windows.bin> [runs] [format] [units]
#include "OrtBench/bench.h"
#include <stdio.h>
#include <stdlib.h>
int main(int argc, char **argv) {
  if (argc < 4) { fprintf(stderr, "usage\n"); return 2; }
  char out[4096];
  int rc = ort_bench(argv[1], argv[2], argc > 5 ? argv[5] : "MLProgram", argc > 6 ? argv[6] : "ALL", "",
                     512, argc > 4 ? atoi(argv[4]) : 4, 4, "", argv[3], out, sizeof out);
  puts(out);
  return rc;
}
