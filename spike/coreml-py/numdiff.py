import sys, numpy as np, onnxruntime as ort
dyn = sys.argv[1]; st = sys.argv[2]; tmp = sys.argv[3]
rng = np.random.default_rng(0)
ids = rng.integers(5, 2000, size=(1, 512)).astype(np.int64); ids[0, 0] = 1
mask = np.ones((1, 512), np.int64)
def run(path, prov):
    so = ort.SessionOptions(); so.log_severity_level = 3
    s = ort.InferenceSession(path, so, providers=prov)
    return s.run(None, {"input_ids": ids, "attention_mask": mask})[0][0]
def cmp(name, a, b):
    cos = (a*b).sum(1)/np.linalg.norm(a,axis=1)/np.linalg.norm(b,axis=1)
    print(f"{name}: max abs {np.abs(a-b).max():.4f} mean abs {np.abs(a-b).mean():.5f} min cos {cos.min():.5f}")
a = run(dyn, ["CPUExecutionProvider"])
cmp("static-fixed-dims (unoptimized) CPU", a, run(tmp, ["CPUExecutionProvider"]))
cml = [("CoreMLExecutionProvider", {"ModelFormat": "MLProgram", "MLComputeUnits": "CPUAndGPU"}), "CPUExecutionProvider"]
cmp("static-folded CoreML GPU", a, run(st, cml))
cmp("static-fixed-dims (unoptimized) CoreML GPU", a, run(tmp, cml))
cmp("dynamic CoreML NN", a, run(dyn, [("CoreMLExecutionProvider", {"ModelFormat": "NeuralNetwork"}), "CPUExecutionProvider"]))
