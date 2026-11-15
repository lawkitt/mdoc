import sys, onnx, onnxruntime as ort
from onnxruntime.tools.onnx_model_utils import make_dim_param_fixed, fix_output_shapes
src, dst, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
m = onnx.load(src)
for i in m.graph.input:
    print(i.name, [d.dim_param or d.dim_value for d in i.type.tensor_type.shape.dim])
dims = {d.dim_param for i in m.graph.input for d in i.type.tensor_type.shape.dim if d.dim_param}
for d in dims:
    v = 1 if "batch" in d.lower() else n
    make_dim_param_fixed(m.graph, d, v); print("fixed", d, v)
fix_output_shapes(m)
tmp = dst + ".tmp.onnx"; onnx.save(m, tmp)
so = ort.SessionOptions(); so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_BASIC
so.optimized_model_filepath = dst
ort.InferenceSession(tmp, so, providers=["CPUExecutionProvider"], disabled_optimizers=["MatMulAddFusion"])
print("saved", dst)
