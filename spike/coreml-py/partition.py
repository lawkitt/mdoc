import sys, time, numpy as np, onnxruntime as ort
path, fmt, units = sys.argv[1], sys.argv[2], sys.argv[3]
so = ort.SessionOptions(); so.log_severity_level = 0 if len(sys.argv) > 4 else 3
so.intra_op_num_threads = 4
prov = [("CoreMLExecutionProvider", {"ModelFormat": fmt, "MLComputeUnits": units})] if fmt != "cpu" else []
t = time.time(); s = ort.InferenceSession(path, so, providers=prov + ["CPUExecutionProvider"]); load = time.time() - t
ids = np.random.randint(5, 1000, size=(1, 512)).astype(np.int64); mask = np.ones((1, 512), np.int64)
ts = []
for _ in range(4):
    t = time.time(); s.run(None, {"input_ids": ids, "attention_mask": mask}); ts.append(time.time() - t)
print(f"{fmt}/{units}: load {load:.1f}s runs {[round(x,3) for x in ts]}")
