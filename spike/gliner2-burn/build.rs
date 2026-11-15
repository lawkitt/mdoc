use burn_onnx::ModelGen;

// The eight GLiNER2-PII fragments the app installs (resources/pseudonymization-model.json).
fn main() {
    let prec = std::env::var("SPIKE_PREC").unwrap_or_else(|_| "fp32".into());
    println!("cargo:rerun-if-env-changed=SPIKE_PREC");
    println!("cargo:rustc-env=SPIKE_PREC={prec}");
    let base = std::env::var("HOME").unwrap()
        + "/Library/Application Support/mdoc/pseudonymization/gliner2-pii-" + &prec + "-e5948986/" + &prec + "_v2/";
    for f in ["token_gather", "schema_gather", "scorer", "classifier", "count_pred_argmax", "span_rep", "count_lstm_fixed", "encoder"] {
        let path = format!("{base}{f}_{prec}.onnx");
        let t = std::time::Instant::now();
        ModelGen::new().input(&path).out_dir("model/").run_from_script();
        println!("cargo:warning=imported {f}_{prec} in {:.1?}", t.elapsed());
        // The generated files carry the precision suffix; give them stable names.
        let out = std::env::var("OUT_DIR").unwrap() + "/model/";
        std::fs::copy(format!("{out}{f}_{prec}.rs"), format!("{out}{f}.rs")).unwrap();
    }
}
