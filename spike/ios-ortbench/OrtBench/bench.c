// SPIKE (lawkitt/mdoc#903): ORT 1.27 C API bench, CPU vs Core ML provider.
#include "bench.h"
#include "onnxruntime_c_api.h"
#include <mach/mach.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec + t.tv_nsec * 1e-9; }

static void footprint(double *cur, double *peak) {
  task_vm_info_data_t info; mach_msg_type_number_t n = TASK_VM_INFO_COUNT;
  if (task_info(mach_task_self(), TASK_VM_INFO, (task_info_t)&info, &n) == KERN_SUCCESS) {
    *cur = info.phys_footprint / 1048576.0; *peak = info.ledger_phys_footprint_peak / 1048576.0;
  } else { *cur = *peak = -1; }
}

#define CHECK(expr) do { OrtStatus *s_ = (expr); if (s_) { snprintf(out, outlen, "{\"error\":\"%s\"}", api->GetErrorMessage(s_)); api->ReleaseStatus(s_); return 1; } } while (0)

int ort_bench(const char *model_path, const char *ep, const char *fmt, const char *units, const char *cache_dir,
              int seq, int runs, int threads, const char *dump_path, char *out, size_t outlen) {
  const OrtApi *api = OrtGetApiBase()->GetApi(ORT_API_VERSION);
  OrtEnv *env; CHECK(api->CreateEnv(ORT_LOGGING_LEVEL_WARNING, "bench", &env));
  OrtSessionOptions *so; CHECK(api->CreateSessionOptions(&so));
  CHECK(api->SetIntraOpNumThreads(so, threads));
  if (strcmp(ep, "coreml") == 0) {
    const char *keys[3] = {"ModelFormat", "MLComputeUnits", "ModelCacheDirectory"};
    const char *vals[3] = {fmt, units, cache_dir};
    CHECK(api->SessionOptionsAppendExecutionProvider(so, "CoreML", keys, vals, (cache_dir && *cache_dir) ? 3 : 2));
  }
  double f0, p0; footprint(&f0, &p0);
  double t = now();
  OrtSession *sess; CHECK(api->CreateSession(env, model_path, so, &sess));
  double load = now() - t;
  OrtAllocator *alloc; CHECK(api->GetAllocatorWithDefaultOptions(&alloc));
  size_t nin, nout; CHECK(api->SessionGetInputCount(sess, &nin)); CHECK(api->SessionGetOutputCount(sess, &nout));
  char *in_names[8]; char *out_names[8]; OrtValue *inputs[8] = {0};
  OrtMemoryInfo *mem; CHECK(api->CreateCpuMemoryInfo(OrtArenaAllocator, OrtMemTypeDefault, &mem));
  int64_t *buf[8]; int64_t shape[2] = {1, seq};
  srand(42);
  for (size_t i = 0; i < nin && i < 8; i++) {
    CHECK(api->SessionGetInputName(sess, i, alloc, &in_names[i]));
    buf[i] = malloc(sizeof(int64_t) * seq);
    for (int j = 0; j < seq; j++)
      buf[i][j] = strstr(in_names[i], "mask") ? 1 : strstr(in_names[i], "type") ? 0 : (j == 0 ? 1 : 5 + rand() % 2000);
    CHECK(api->CreateTensorWithDataAsOrtValue(mem, buf[i], sizeof(int64_t) * seq, shape, 2,
                                              ONNX_TENSOR_ELEMENT_DATA_TYPE_INT64, &inputs[i]));
  }
  for (size_t i = 0; i < nout && i < 8; i++) CHECK(api->SessionGetOutputName(sess, i, alloc, &out_names[i]));
  char times[512] = ""; size_t tl = 0; double best = 1e9;
  for (int r = 0; r < runs; r++) {
    OrtValue *outputs[8] = {0};
    t = now();
    CHECK(api->Run(sess, NULL, (const char *const *)in_names, (const OrtValue *const *)inputs, nin,
                   (const char *const *)out_names, nout, outputs));
    double dt = now() - t; if (dt < best) best = dt;
    tl += snprintf(times + tl, sizeof(times) - tl, "%s%.3f", r ? "," : "", dt);
    if (r == runs - 1 && dump_path && *dump_path) {
      float *data; CHECK(api->GetTensorMutableData(outputs[0], (void **)&data));
      OrtTensorTypeAndShapeInfo *info; CHECK(api->GetTensorTypeAndShape(outputs[0], &info));
      size_t count; CHECK(api->GetTensorShapeElementCount(info, &count));
      FILE *f = fopen(dump_path, "wb"); if (f) { fwrite(data, sizeof(float), count, f); fclose(f); }
      api->ReleaseTensorTypeAndShapeInfo(info);
    }
    for (size_t i = 0; i < nout; i++) api->ReleaseValue(outputs[i]);
  }
  double f1, p1; footprint(&f1, &p1);
  snprintf(out, outlen,
           "{\"model\":\"%s\",\"ep\":\"%s\",\"format\":\"%s\",\"units\":\"%s\",\"seq\":%d,\"threads\":%d,\"load_s\":%.2f,"
           "\"runs_s\":[%s],\"best_s\":%.3f,\"footprint_start_mb\":%.0f,\"footprint_end_mb\":%.0f,\"footprint_peak_mb\":%.0f}",
           strrchr(model_path, '/') ? strrchr(model_path, '/') + 1 : model_path, ep, fmt, units, seq, threads, load,
           times, best, f0, f1, p1);
  return 0;
}
