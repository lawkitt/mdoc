import sys, numpy as np, onnxruntime as ort
dyn, st = sys.argv[1], sys.argv[2]
mask = np.ones((1, 512), np.int64)
so = ort.SessionOptions(); so.log_severity_level = 3
for label, ids in [("C rand ids", np.fromfile("ids.bin", dtype=np.int64).reshape(1, 512)),
                   ("numpy ids", np.random.default_rng(0).integers(5, 2000, size=(1, 512)).astype(np.int64))]:
    ref = ort.InferenceSession(dyn, so, providers=["CPUExecutionProvider"]).run(None, {"input_ids": ids, "attention_mask": mask})[0][0]
    b = ort.InferenceSession(st, so, providers=[("CoreMLExecutionProvider", {"ModelFormat": "MLProgram", "MLComputeUnits": "CPUAndGPU"}), "CPUExecutionProvider"]).run(None, {"input_ids": ids, "attention_mask": mask})[0][0]
    cos = (ref*b).sum(1)/np.linalg.norm(ref,axis=1)/np.linalg.norm(b,axis=1)
    bad = np.where(cos < 0.999)[0]
    print(label, "rows cos<0.999:", len(bad), "first/last", bad[:5], bad[-5:], "median cos", float(np.median(cos)))
