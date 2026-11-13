//! The recognition fixtures on their snapshots (ADR 0093), on any OS: each
//! fixture's committed raw model output through the correction stage and the
//! review, against its gold and status. The Mac's detection gate
//! (`just recognize --check`) also reruns the model to keep snapshots fresh.
use mdoc_detect::recognition::{Comparison, Fixture, Snapshot, fixture_paths, proposals, verdict};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn every_recognition_fixture_is_as_its_status_says() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/recognition");
    let paths = fixture_paths(&root).unwrap();
    assert!(
        paths.len() >= 40,
        "fixtures missing under {}",
        root.display()
    );
    let config = mdoc_store::settings::PiiConfig::default();
    let model = mdoc_detect::model_name(mdoc_store::settings::PiiModel::Fp16);
    let check = |path: &Path| -> Result<(), String> {
        let fixture = Fixture::read(path)?;
        fixture.validate()?;
        fixture.validate_layout()?;
        let snapshot = Snapshot::read(&fixture.snapshot_path())?;
        if snapshot.model != model || snapshot.threshold != config.threshold {
            return Err(format!(
                "snapshot made with {} at {}, not {model} at {}; refresh it (just recognize --refresh-snapshots, then CI's with just snapshots-fetch)",
                snapshot.model, snapshot.threshold, config.threshold
            ));
        }
        snapshot.validate(&fixture.source)?;
        let proposed = proposals(&fixture.source, &snapshot.entities)?;
        let comparison = Comparison::new(&fixture.gold, &proposed);
        verdict(&fixture, &comparison).map_or(Ok(()), Err)
    };
    // The fixtures are independent and share the rule pack, so they are
    // checked on every core: one after another, the loop took 15 to 17 s on
    // hosted macOS and 20 s on the PC (ADR 0112's sweep).
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut failed: Vec<(usize, String)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers.min(paths.len()))
            .map(|_| {
                scope.spawn(|| {
                    let mut failed = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = paths.get(index) else {
                            break failed;
                        };
                        if let Err(error) = check(path) {
                            failed.push((index, format!("{}: {error}", path.display())));
                        }
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|e| std::panic::resume_unwind(e))
            })
            .collect()
    });
    failed.sort();
    let mut failures: Vec<String> = failed.into_iter().map(|(_, failure)| failure).collect();
    // A snapshot whose fixture was renamed or removed.
    let mut pending = vec![root.clone()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && let Some(id) = name.strip_suffix(".model.json")
                && !path.with_file_name(format!("{id}.md")).exists()
            {
                failures.push(format!("{}: snapshot without a fixture", path.display()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} recognition fixtures (just recognize <file> --gold --snapshot shows one):\n{}",
        failures.len(),
        paths.len(),
        failures.join("\n")
    );
}

/// #366 (U27): in the Russian demo, from its snapshot, every form of
/// Павлова Марина Сергеевна the review proposes is one identity, and her
/// husband and her lawyer keep their own (ADR 0092 propagation).
/// "Павлову" alone is her accusative and his dative: either, so its own.
#[test]
fn the_russian_demo_names_pavlova_once() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/recognition/ru/contract/demo-settlement-ru.md");
    let fixture = Fixture::read(&path).unwrap();
    let snapshot = Snapshot::read(&fixture.snapshot_path()).unwrap();
    let source = fixture.source.as_str();
    let mut review = mdoc_pii::Review::default();
    review
        .ingest(source, mdoc_detect::correct(source, &snapshot.entities))
        .unwrap();
    let identity = |span: &mdoc_detect::recognition::Span| {
        let candidate = review
            .candidates()
            .iter()
            .find(|c| c.range == span.range)
            .unwrap_or_else(|| panic!("{} is not proposed", &source[span.range.clone()]));
        review
            .occurrence_identity(candidate.variant, &candidate.range)
            .unwrap()
    };
    let others = [
        "Павлову Дмитрию Андреевичу",
        "Шахова Ирина Олеговна",
        "Павлову",
    ];
    let people = fixture.gold.iter().filter(|span| span.category == "PERSON");
    let (theirs, hers): (Vec<_>, Vec<_>) =
        people.partition(|span| others.contains(&&source[span.range.clone()]));
    assert_eq!(hers.len(), 8, "her forms in the gold");
    let mut ids: Vec<u64> = hers.iter().map(|span| identity(span)).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 1, "one identity for her: {ids:?}");
    let mut all: Vec<u64> = theirs.iter().map(|span| identity(span)).collect();
    all.push(ids[0]);
    all.sort_unstable();
    all.dedup();
    assert_eq!(
        all.len(),
        4,
        "her husband, her lawyer and \"Павлову\" are others"
    );
    // No alias is spent on a form that joined her.
    let mut aliases: Vec<_> = all
        .iter()
        .map(|id| review.identity(*id).unwrap().alias.clone())
        .collect();
    aliases.sort();
    assert_eq!(aliases, ["PERSON_1", "PERSON_2", "PERSON_3", "PERSON_4"]);
}
