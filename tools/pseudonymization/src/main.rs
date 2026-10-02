use anyhow::{Context, Result, bail, ensure};
use mdoc_pseudonymization_qualification::{model, setup, summarize};
use std::{
    fs,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

fn wait_child(
    child: &mut Child,
    started: Instant,
    deadline: Duration,
) -> Result<(ExitStatus, bool)> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status, false));
        }
        if started.elapsed() >= deadline {
            // A synchronous ORT call need not cooperate with cancellation.
            child.kill()?;
            return Ok((child.wait()?, true));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.first().and_then(|s| s.to_str()) {
        Some("summarize") if args.len() > 1 => {
            print!(
                "{}",
                summarize(&args[1..].iter().map(Into::into).collect::<Vec<_>>())?
            );
            Ok(())
        }
        Some("setup") if args.len() == 3 => setup(
            &model(args[1].to_str().context("invalid candidate")?)?,
            Path::new(&args[2]),
        ),
        Some("run") if (8..=10).contains(&args.len()) => {
            let threshold = args.get(8).and_then(|a| a.to_str()).unwrap_or("0.5");
            let deadline: u64 = args
                .get(9)
                .and_then(|a| a.to_str())
                .unwrap_or("600")
                .parse()?;
            ensure!(
                (1..=3600).contains(&deadline),
                "deadline must be 1..3600 seconds"
            );
            let output = Path::new(&args[5]);
            // Do not mistake an earlier successful run for this run's result.
            ensure!(
                !output.exists(),
                "output already exists: {}",
                output.display()
            );
            let parent = output
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            fs::create_dir_all(parent)?;
            let stderr = output.with_extension("stderr.log");
            let log = fs::File::create(&stderr)?;
            let started = Instant::now();
            let mut child = Command::new(&args[1])
                .args([&args[2], &args[3], &args[4], &args[5], &args[6]])
                .arg(threshold)
                .env("ORT_DYLIB_PATH", &args[6])
                .env("TOKENIZERS_PARALLELISM", "false")
                .env("GLINER2_DEVICE", "cpu")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(log)
                .spawn()
                .context("starting isolated inference adapter")?;
            let (status, timed_out) =
                wait_child(&mut child, started, Duration::from_secs(deadline))?;
            fs::write(
                output.with_extension("process.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "success": status.success(), "exit_code": status.code(), "deadline_exceeded": timed_out,
                    "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
                    "report_written": output.exists(), "stderr": stderr,
                    "machine": args[7].to_string_lossy(),
                }))?,
            )?;
            if !status.success() {
                bail!(
                    "adapter failed (deadline exceeded: {timed_out}); see {}",
                    stderr.display()
                );
            }
            ensure!(
                output.exists(),
                "adapter exited successfully without a report"
            );
            Ok(())
        }
        _ => bail!(
            "Usage:\n  setup CANDIDATE CACHE\n  run ADAPTER CANDIDATE CACHE CORPUS OUTPUT RUNTIME MACHINE [THRESHOLD] [DEADLINE_SECONDS]\n  summarize REPORT..."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "child-process fixture for deadline test"]
    fn idle_worker() {
        std::thread::sleep(Duration::from_secs(10));
    }

    #[test]
    fn deadline_kills_and_reaps_uncooperative_worker() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::idle_worker", "--ignored"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let (status, timed_out) =
            wait_child(&mut child, started, Duration::from_millis(100)).unwrap();
        assert!(timed_out);
        assert!(!status.success());
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn successful_exit_is_not_cancellation() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let (status, timed_out) =
            wait_child(&mut child, Instant::now(), Duration::from_secs(3)).unwrap();
        assert!(status.success());
        assert!(!timed_out);
    }
}
