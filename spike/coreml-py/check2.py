import sys, numpy as np, onnxruntime as ort
dyn, st = sys.argv[1], sys.argv[2]
mask = np.ones((1, 512), np.int64)
C = np.fromfile("ids.bin", dtype=np.int64).reshape(1, 512)
so = ort.SessionOptions(); so.log_severity_level = 3
ref = ort.InferenceSession(dyn, so, providers=["CPUExecutionProvider"]).run(None, {"input_ids": C, "attention_mask": mask})[0][0]
def cos(b): c=(ref*b).sum(1)/np.linalg.norm(ref,axis=1)/np.linalg.norm(b,axis=1); return round(float(c.min()),5)
print("v2 CPU", cos(ort.InferenceSession(st, so, providers=["CPUExecutionProvider"]).run(None, {"input_ids": C, "attention_mask": mask})[0][0]))
for i in range(2):
    s = ort.InferenceSession(st, so, providers=[("CoreMLExecutionProvider", {"ModelFormat": "MLProgram", "MLComputeUnits": "CPUAndGPU"}), "CPUExecutionProvider"])
    print("v2 CoreML GPU session", i, cos(s.run(None, {"input_ids": C, "attention_mask": mask})[0][0]))
