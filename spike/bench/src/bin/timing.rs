//! SPIKE (#903): time the model stage over one text, as a scan runs it.
//! `timing <text file> [runs]`; env: SPIKE_MODEL=fp16|fp32, GLINER2_DEVICE=cpu|coreml,
//! GLINER2_BURN=flex|cpu|metal. Prints load time, each run, and peak RSS.
use std::time::Instant;

fn peak_rss_mb() -> f64 {
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    u.ru_maxrss as f64 / (1024.0 * 1024.0) // bytes on macOS
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).expect("text");
    let runs: usize = args.get(2).and_then(|r| r.parse().ok()).unwrap_or(3);
    let config = mdoc_store::settings::PiiConfig {
        model: match std::env::var("SPIKE_MODEL").as_deref() {
            Ok("fp32") => mdoc_store::settings::PiiModel::Fp32,
            _ => mdoc_store::settings::PiiModel::Fp16,
        },
        ..Default::default()
    };
    let root = mdoc_detect::root_for(config.model).unwrap();
    let runtime = mdoc_import::ocr::onnx_runtime(false).unwrap();
    let before = peak_rss_mb();
    let t = Instant::now();
    let mut session = mdoc_detect::ModelSession::load(&root, &runtime, &config).unwrap();
    let load = t.elapsed();
    let after_load = peak_rss_mb();
    let mut times = Vec::new();
    let mut count = 0;
    for _ in 0..runs {
        let t = Instant::now();
        let e = session.entities(&text).unwrap();
        times.push(t.elapsed().as_secs_f64());
        count = e.len();
    }
    let chars = text.chars().count();
    let best = times.iter().cloned().fold(f64::INFINITY, f64::min);
    let per10k = |s: f64| s * 10_000.0 / chars as f64;
    println!(
        "chars={chars} entities={count} load={:.2}s runs={:?} first_per10k={:.2}s best_per10k={:.2}s rss_start={before:.0}MB rss_after_load={after_load:.0}MB peak_rss={:.0}MB",
        load.as_secs_f64(),
        times.iter().map(|t| (t * 100.0).round() / 100.0).collect::<Vec<_>>(),
        per10k(times[0]),
        per10k(best),
        peak_rss_mb(),
    );
}
