//! `just recognize`: the recognition fixtures' tool (ADR 0093). Not shipped.
//!
//! ```text
//! just recognize <file.md|folder> [--gold] [--snapshot]
//! just recognize --scoreboard [<folder>] [--snapshot]
//! just recognize --refresh-snapshots [--shard <k>/<n>] [<file.md|folder>]
//! just recognize --check [--shard <k>/<n>] [--out <dir>] [<folder>]
//! just recognize --record-flaws [<file.md|folder>]
//! just recognize --setup
//! ```
//!
//! A file prints its proposals marked as gold is (`⟦CATEGORY|text⟧`);
//! `--gold` prints each fixture's misses, false hits and boundary errors
//! and fails when one is not as its status expects; a document without a
//! status (the holdout) is expected to pass, and without a snapshot the
//! model runs. The model is the
//! installed pinned FP16 from the app data (read only, never downloaded;
//! `MDOC_PII_MODEL_ROOT` and `MDOC_ONNX_RUNTIME` point elsewhere);
//! `--snapshot` reads each fixture's committed `<id>.model.json` instead.
//! `--scoreboard` prints recall and precision per language × category ×
//! kind, and exits 0 whatever the documents did. `--refresh-snapshots` rewrites snapshots (on the Mac, the
//! reference platform). `--check` is the detection gate: it reruns the model
//! over every fixture, fails on a stale snapshot or an unexpected result,
//! and on a model run over its 5-minute budget. `--shard k/n` checks (or
//! refreshes) every n-th fixture from the k-th (1-based) in sorted order,
//! one CI shard, and `--out` writes each stale or missing snapshot afresh under that folder at
//! its path in the repository (CI's `detection-snapshots-<k>` artifact, which
//! `just snapshots-fetch` applies; ADR 0095). `--setup` installs the pinned
//! FP16 and the ONNX Runtime into the app data through mdoc's own verified
//! setup, for CI's runner; no other mode downloads. `--record-flaws` writes each
//! known-flaw fixture's flaws, from its snapshot, as its `expect:` lines:
//! a known-flaw fixture fails on any flaw it has not recorded (the
//! precision ratchet). Folders default to
//! `tests/fixtures/recognition`; any folder of fixtures works (the holdout).
use mdoc_detect::{
    ModelSession, RawEntity,
    recognition::{
        Comparison, Fixture, Scoreboard, Snapshot, Status, annotate, fixture_paths, proposals,
        verdict, with_expected,
    },
};
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

const BUDGET: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Annotate,
    Gold,
    Scoreboard,
    Refresh,
    Check,
    Record,
    Setup,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn usage() -> String {
    "usage: just recognize <file.md|folder> [--gold] [--snapshot]\n       \
     just recognize --scoreboard [<folder>] [--snapshot]\n       \
     just recognize --refresh-snapshots [--shard <k>/<n>] [<file.md|folder>]\n       \
     just recognize --check [--shard <k>/<n>] [--out <dir>] [<folder>]\n       \
     just recognize --record-flaws [<file.md|folder>]\n       \
     just recognize --setup"
        .into()
}

fn run() -> Result<bool, String> {
    let mut mode = Mode::Annotate;
    let mut gold = false;
    let mut from_snapshot = false;
    let mut target = None;
    let mut shard = None;
    let mut out = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--gold" => gold = true,
            "--snapshot" => from_snapshot = true,
            "--scoreboard" => mode = Mode::Scoreboard,
            "--refresh-snapshots" => mode = Mode::Refresh,
            "--check" => mode = Mode::Check,
            "--record-flaws" => mode = Mode::Record,
            "--setup" => mode = Mode::Setup,
            "--shard" => shard = Some(parse_shard(&args.next().unwrap_or_default())?),
            "--out" => out = Some(PathBuf::from(args.next().ok_or_else(usage)?)),
            "-h" | "--help" => {
                println!("{}", usage());
                return Ok(true);
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag {flag}\n{}", usage()));
            }
            path if target.is_none() => target = Some(PathBuf::from(path)),
            _ => return Err(usage()),
        }
    }
    if mode == Mode::Annotate && gold {
        mode = Mode::Gold;
    }
    if shard.is_some() && !matches!(mode, Mode::Check | Mode::Refresh) {
        return Err(format!(
            "--shard goes with --check or --refresh-snapshots\n{}",
            usage()
        ));
    }
    if out.is_some() && mode != Mode::Check {
        return Err(format!("--out goes with --check\n{}", usage()));
    }
    if mode == Mode::Setup {
        return setup().map(|()| true);
    }
    if from_snapshot && matches!(mode, Mode::Refresh | Mode::Check) {
        return Err(
            "--snapshot reads snapshots; --refresh-snapshots and --check run the model".into(),
        );
    }
    let root = match (target, mode) {
        (Some(path), _) => path,
        (None, Mode::Annotate | Mode::Gold) => return Err(usage()),
        (None, _) => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/recognition")
            .canonicalize()
            .map_err(|e| format!("tests/fixtures/recognition: {e}"))?,
    };
    let mut paths = fixture_paths(&root)?;
    if paths.is_empty() {
        return Err(format!("no fixtures under {}", root.display()));
    }
    if let Some((k, n)) = shard {
        paths = paths.into_iter().skip(k - 1).step_by(n).collect();
    }
    let mut model = if from_snapshot || mode == Mode::Record {
        None
    } else {
        Some(Model::load()?)
    };
    match mode {
        Mode::Annotate => {
            let mut ok = true;
            for path in &paths {
                let fixture = Fixture::read(path)?;
                let raw = entities(&fixture, model.as_mut())?;
                match proposals(&fixture.source, &raw) {
                    Ok(spans) => {
                        if paths.len() > 1 {
                            println!("== {}", path.display());
                        }
                        print!("{}", annotate(&fixture.source, &spans));
                        if !fixture.source.ends_with('\n') {
                            println!();
                        }
                    }
                    Err(error) => {
                        eprintln!("{}: {error}", path.display());
                        ok = false;
                    }
                }
            }
            Ok(ok)
        }
        Mode::Gold | Mode::Scoreboard => {
            let mut scoreboard = Scoreboard::default();
            let mut ok = true;
            for path in &paths {
                let fixture = Fixture::read(path)?;
                let raw = entities(&fixture, model.as_mut())?;
                let proposed = proposals(&fixture.source, &raw)?;
                let comparison = Comparison::new(&fixture.gold, &proposed);
                scoreboard.add(&fixture, &comparison, &proposed);
                let unexpected = verdict(&fixture, &comparison);
                ok &= unexpected.is_none();
                if mode == Mode::Gold {
                    let status = fixture.status.map_or("no status: pass", Status::name);
                    let result = if comparison.passes() {
                        "passes"
                    } else {
                        "fails"
                    };
                    let mark = if unexpected.is_some() {
                        "UNEXPECTED"
                    } else {
                        "ok"
                    };
                    println!("{mark} {} ({status}): {result}", path.display());
                    match unexpected {
                        Some(why) => println!("  {}", why.trim_end()),
                        None => print!("{}", comparison.report(&fixture.source)),
                    }
                }
            }
            if mode == Mode::Scoreboard {
                // A report: aggregates only, whatever each document did.
                print!("{}", scoreboard.render());
                return Ok(true);
            }
            Ok(ok)
        }
        Mode::Refresh => {
            let model = model.as_mut().expect("model mode");
            let started = Instant::now();
            for path in &paths {
                let fixture = Fixture::read(path)?;
                let snapshot = model.snapshot(&fixture.source)?;
                std::fs::write(fixture.snapshot_path(), snapshot.to_json())
                    .map_err(|e| format!("{}: {e}", fixture.snapshot_path().display()))?;
                println!(
                    "{}: {} entities",
                    fixture.snapshot_path().display(),
                    snapshot.entities.len()
                );
            }
            println!(
                "{} snapshots in {:.1}s",
                paths.len(),
                started.elapsed().as_secs_f64()
            );
            Ok(true)
        }
        Mode::Record => {
            let mut recorded = 0;
            for path in &paths {
                let fixture = Fixture::read(path)?;
                if fixture.status != Some(Status::KnownFlaw) {
                    continue;
                }
                let raw = entities(&fixture, None)?;
                let comparison = Comparison::new(&fixture.gold, &proposals(&fixture.source, &raw)?);
                if comparison.passes() {
                    println!("{}: passes now: flip its status to pass", path.display());
                    continue;
                }
                let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                let lines = comparison.report_lines(&fixture.source);
                std::fs::write(path, with_expected(&text, &lines)?)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                recorded += 1;
            }
            println!("recorded the flaws of {recorded} known-flaw fixtures");
            Ok(true)
        }
        Mode::Check => {
            let model = model.as_mut().expect("model mode");
            let started = Instant::now();
            let mut failures = Vec::new();
            for path in &paths {
                let fixture = Fixture::read(path)?;
                let mut problems = Vec::new();
                if let Err(error) = fixture.validate().and_then(|()| fixture.validate_layout()) {
                    problems.push(error);
                }
                let fresh = model.snapshot(&fixture.source)?;
                let stale = match Snapshot::read(&fixture.snapshot_path()) {
                    Ok(saved) => saved
                        .stale(&fresh)
                        .map(|why| format!("stale snapshot ({why})")),
                    Err(error) => Some(format!("no snapshot: {error}")),
                };
                if let Some(why) = stale {
                    problems.push(match &out {
                        Some(out) => {
                            write_fresh(out, &fixture, &fresh)?;
                            format!("{why}: its fresh snapshot is under {}", out.display())
                        }
                        None => format!(
                            "{why}: run just recognize --refresh-snapshots {}",
                            path.display()
                        ),
                    });
                }
                let proposed = proposals(&fixture.source, &fresh.entities)?;
                let comparison = Comparison::new(&fixture.gold, &proposed);
                if let Some(why) = verdict(&fixture, &comparison) {
                    problems.push(why);
                }
                if !problems.is_empty() {
                    failures.push(format!("{}: {}", path.display(), problems.join("; ")));
                }
            }
            let elapsed = started.elapsed();
            let part = shard.map_or(String::new(), |(k, n)| format!(" (shard {k} of {n})"));
            println!(
                "detection gate{part}: {} fixtures, model run {:.1}s (budget {}s)",
                paths.len(),
                elapsed.as_secs_f64(),
                BUDGET.as_secs()
            );
            if elapsed > BUDGET {
                failures.push(format!(
                    "the model run took {:.0}s, over its {}s budget: raise the detection gate's shards (ADR 0095)",
                    elapsed.as_secs_f64(),
                    BUDGET.as_secs()
                ));
            } else if elapsed > BUDGET.mul_f64(0.8) {
                // A GitHub Actions annotation, plain text elsewhere: the
                // signal to add a shard before the budget fails (ADR 0100).
                println!(
                    "::warning::detection gate{part}: the model run took {:.0}s, over 80% of its {}s budget: add a shard (queue.yml, nightly.yml)",
                    elapsed.as_secs_f64(),
                    BUDGET.as_secs()
                );
            }
            for failure in &failures {
                println!("FAIL {failure}");
            }
            Ok(failures.is_empty())
        }
        Mode::Setup => unreachable!("set up above"),
    }
}

/// `k/n`: the k-th of n shards, 1-based.
fn parse_shard(text: &str) -> Result<(usize, usize), String> {
    let parsed = text
        .split_once('/')
        .and_then(|(k, n)| Some((k.parse::<usize>().ok()?, n.parse::<usize>().ok()?)));
    match parsed {
        Some((k, n)) if (1..=n).contains(&k) => Ok((k, n)),
        _ => Err(format!(
            "--shard takes <k>/<n> with 1 <= k <= n, not {text:?}"
        )),
    }
}

/// The fixture's fresh snapshot under `out`, at its path in the repository
/// (or by its file name, for a fixture outside it).
fn write_fresh(out: &Path, fixture: &Fixture, fresh: &Snapshot) -> Result<(), String> {
    let saved = std::path::absolute(fixture.snapshot_path()).map_err(|e| e.to_string())?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let relative = match saved.strip_prefix(&repo) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => saved.file_name().map(PathBuf::from).unwrap_or_default(),
    };
    let path = out.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(&path, fresh.to_json()).map_err(|e| format!("{}: {e}", path.display()))
}

/// The pinned FP16 and the shared ONNX Runtime, downloaded and verified as
/// mdoc's own setup does them (CI's detection gate; ADR 0095). Files already
/// in place and verified are kept.
fn setup() -> Result<(), String> {
    let config = mdoc_store::settings::PiiConfig {
        model: mdoc_store::settings::PiiModel::Fp16,
        ..Default::default()
    };
    let started = Instant::now();
    mdoc_detect::setup_config(&config, &mdoc_import::model_download::Progress::default())
        .map_err(|e| e.to_string())?;
    let root = mdoc_detect::root_for(config.model).map_err(|e| e.to_string())?;
    println!(
        "set up {} in {} ({:.1}s)",
        mdoc_detect::model_name(config.model),
        root.display(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// The fixture's raw entities: from the model, or its snapshot.
fn entities(fixture: &Fixture, model: Option<&mut Model>) -> Result<Vec<RawEntity>, String> {
    match model {
        Some(model) => Ok(model.snapshot(&fixture.source)?.entities),
        None => {
            let snapshot = Snapshot::read(&fixture.snapshot_path())?;
            snapshot
                .validate(&fixture.source)
                .map_err(|e| format!("{}: {e}", fixture.path.display()))?;
            Ok(snapshot.entities)
        }
    }
}

struct Model {
    session: ModelSession,
    config: mdoc_store::settings::PiiConfig,
}

impl Model {
    /// The pinned FP16 at the default threshold, as installed by the app.
    fn load() -> Result<Self, String> {
        let config = mdoc_store::settings::PiiConfig {
            model: mdoc_store::settings::PiiModel::Fp16,
            ..Default::default()
        };
        let root = match std::env::var_os("MDOC_PII_MODEL_ROOT") {
            Some(root) => PathBuf::from(root),
            None => mdoc_detect::root_for(config.model).map_err(|e| e.to_string())?,
        };
        let runtime = match std::env::var_os("MDOC_ONNX_RUNTIME") {
            Some(runtime) => PathBuf::from(runtime),
            None => mdoc_import::ocr::onnx_runtime(false).map_err(|e| {
                format!("no verified ONNX Runtime in the app data ({e}); set up the model in mdoc or set MDOC_ONNX_RUNTIME")
            })?,
        };
        let session = ModelSession::load(&root, &runtime, &config).map_err(|e| {
            format!(
                "{e} (model {}: set up {} in mdoc, or set MDOC_PII_MODEL_ROOT; --snapshot reads committed snapshots instead)",
                root.display(),
                mdoc_detect::model_name(config.model)
            )
        })?;
        Ok(Self { session, config })
    }

    fn snapshot(&mut self, source: &str) -> Result<Snapshot, String> {
        Ok(Snapshot {
            model: mdoc_detect::model_name(self.config.model),
            threshold: self.config.threshold,
            entities: self.session.entities(source).map_err(|e| e.to_string())?,
        })
    }
}
