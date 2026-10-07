#[path = "baseline_policy.rs"]
mod policy;
use policy::*;
use std::time::Instant;
fn main() {
    for groups in [1, 1000, 20000] {
        let mut source = String::new();
        let mut detections = Vec::new();
        for n in 0..20000 {
            let start = source.len();
            source.push_str(&format!("Name{:05}", n % groups));
            detections.push(Detection {
                range: start..source.len(),
                category: Category::Person,
                score: 0.9,
            });
            source.push_str(" ordinary prose.\n");
        }
        source.push_str(&" ".repeat(2097152 - source.len()));
        let mut r = Review::default();
        let start = Instant::now();
        r.ingest(&source, detections).unwrap();
        let ingest = start.elapsed();
        let start = Instant::now();
        r.refresh(&source);
        let refresh = start.elapsed();
        let start = Instant::now();
        let plan = r.plan_all(&source, None).unwrap();
        let planning = start.elapsed();
        println!(
            "groups={groups}, source={}, mentions={}, ingest_ms={:.3}, refresh_ms={:.3}, plan_ms={:.3}",
            source.len(),
            plan.len(),
            ingest.as_secs_f64() * 1000.,
            refresh.as_secs_f64() * 1000.,
            planning.as_secs_f64() * 1000.
        );
    }
}
