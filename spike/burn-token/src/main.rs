//! `burn-token <windows.json> <out.bin> [runs]`: windows.json = [{"input_ids":[..],"attention_mask":[..]},..].
//! BURN=flex|metal. Writes every window's logits (f32, concatenated) and prints timing JSON.
#[allow(unused, clippy::all)]
mod token {
    include!(concat!(env!("OUT_DIR"), "/model/token.rs"));
}
use burn::prelude::*;
use burn::tensor::TensorData;
use std::time::Instant;

fn footprint_peak_mb() -> f64 {
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    u.ru_maxrss as f64 / 1048576.0
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let windows: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    let runs: usize = a.get(3).and_then(|r| r.parse().ok()).unwrap_or(3);
    let device = match std::env::var("BURN").as_deref() {
        Ok("metal") => Device::metal(Default::default()),
        _ => Device::flex(),
    };
    let t = Instant::now();
    let model = token::Model::from_file(env!("BURN_TOKEN_BPK"), &device);
    let load = t.elapsed().as_secs_f64();
    let ints = |v: &serde_json::Value| -> Vec<i64> { v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect() };
    let mut times = Vec::new();
    let mut dump: Vec<f32> = Vec::new();
    for r in 0..runs {
        let t = Instant::now();
        for w in &windows {
            let ids = ints(&w["input_ids"]);
            let mask = ints(&w["attention_mask"]);
            let n = ids.len();
            let ids = Tensor::<2, Int>::from_data(TensorData::new(ids, [1, n]), &device);
            let mask = Tensor::<2, Int>::from_data(TensorData::new(mask, [1, n]), &device);
            let logits = model.forward(ids, mask);
            let data = logits.into_data().convert::<f32>();
            if r == 0 {
                dump.extend(data.iter::<f32>());
            }
        }
        times.push(t.elapsed().as_secs_f64());
    }
    let bytes: Vec<u8> = dump.iter().flat_map(|f| f.to_le_bytes()).collect();
    std::fs::write(&a[2], bytes).unwrap();
    println!(
        "{{\"backend\":\"{}\",\"windows\":{},\"load_s\":{load:.2},\"runs_s\":{:?},\"peak_rss_mb\":{:.0}}}",
        std::env::var("BURN").unwrap_or("flex".into()),
        windows.len(),
        times.iter().map(|t| (t * 1000.0).round() / 1000.0).collect::<Vec<_>>(),
        footprint_peak_mb()
    );
}
