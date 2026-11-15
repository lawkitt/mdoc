use burn_onnx::ModelGen;
// BURN_TOKEN_ONNX=<model.onnx>: the token classifier to import.
fn main() {
    println!("cargo:rerun-if-env-changed=BURN_TOKEN_ONNX");
    let path = std::env::var("BURN_TOKEN_ONNX").expect("BURN_TOKEN_ONNX");
    let t = std::time::Instant::now();
    ModelGen::new().input(&path).out_dir("model/").run_from_script();
    let stem = std::path::Path::new(&path).file_stem().unwrap().to_string_lossy().into_owned();
    let out = std::env::var("OUT_DIR").unwrap() + "/model/";
    std::fs::copy(format!("{out}{stem}.rs"), format!("{out}token.rs")).unwrap();
    println!("cargo:rustc-env=BURN_TOKEN_BPK={out}{stem}.bpk");
    println!("cargo:warning=imported {path} in {:.1?}", t.elapsed());
}
