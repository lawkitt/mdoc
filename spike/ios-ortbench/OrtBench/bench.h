#pragma once
#include <stddef.h>
// Runs one ONNX model `runs` times on [1, seq] inputs. ep: "cpu" or "coreml".
// coreml_format: "MLProgram"|"NeuralNetwork"; coreml_units: "ALL"|"CPUAndNeuralEngine"|"CPUAndGPU"|"CPUOnly".
// Writes a one-line JSON result (or error) to out. Optionally dumps the first output as float32 to dump_path.
int ort_bench(const char *model_path, const char *ep, const char *coreml_format, const char *coreml_units,
              const char *cache_dir, int seq, int runs, int threads, const char *dump_path, char *out, size_t outlen);
