// App quit and the conversion worker (#925 decision 10), in a binary of its
// own: the quit flag stays set for the rest of the process.
use mdoc_import::{
    conversion_worker::{convert_with_command, shutdown_workers},
    import::ImportError,
};
use std::{
    path::Path,
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn sleeping_worker() -> Command {
    let mut command = Command::new(Path::new(env!("CARGO_BIN_EXE_mdoc")));
    command.env("MDOC_CONVERSION_WORKER_FAULT", "sleep");
    command
}

/// Quit kills and reaps a running worker before it returns, and no worker
/// starts after it.
#[test]
fn quit_stops_running_workers_and_starts_no_more() {
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let result = std::thread::scope(|scope| {
        let converting = scope.spawn(|| {
            convert_with_command(
                b"a,b\n".to_vec(),
                Some("csv"),
                &cancel,
                sleeping_worker(),
                Duration::from_secs(100),
            )
        });
        std::thread::sleep(Duration::from_millis(500));
        let quitting = Instant::now();
        shutdown_workers();
        // Within its wait, so it saw no supervision left: the worker was
        // killed and reaped before it returned.
        assert!(quitting.elapsed() < Duration::from_millis(1500));
        converting.join().unwrap()
    });
    assert!(matches!(result, Err(ImportError::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(60));
    let after = convert_with_command(
        b"a,b\n".to_vec(),
        Some("csv"),
        &cancel,
        sleeping_worker(),
        Duration::from_secs(100),
    );
    assert!(matches!(after, Err(ImportError::Cancelled)), "{after:?}");
}
