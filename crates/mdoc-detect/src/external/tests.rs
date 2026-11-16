use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn pin(bytes: &[u8]) -> Pin {
    Pin {
        bytes: bytes.len() as u64,
        sha256: sha(bytes),
    }
}

/// A protobuf field: number and encoded payload.
fn len_field(number: u64, payload: &[u8]) -> Vec<u8> {
    let mut bytes = header(number, payload.len() as u64);
    bytes.extend(payload);
    bytes
}

fn varint_field(number: u64, value: u64) -> Vec<u8> {
    let mut bytes = varint(number << 3);
    bytes.extend(varint(value));
    bytes
}

fn entry(key: &str, value: &str) -> Vec<u8> {
    let mut bytes = len_field(1, key.as_bytes());
    bytes.extend(len_field(2, value.as_bytes()));
    len_field(13, &bytes)
}

/// A tensor's fields before and after: `raw` moves out at `offset` when
/// it is at least the threshold.
fn tensor(name: &str, raw: &[u8], offset: u64, tail: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut head = varint_field(1, 4); // dims: [4]
    head.extend(varint_field(2, 10)); // FLOAT16
    head.extend(len_field(8, name.as_bytes()));
    let mut before = head.clone();
    before.extend(len_field(9, raw));
    before.extend(tail);
    let mut after = head;
    if raw.len() as u64 >= MIN_EXTERNAL_BYTES {
        after.extend(entry("location", "encoder.onnx.data"));
        after.extend(entry("offset", &offset.to_string()));
        after.extend(entry("length", &raw.len().to_string()));
        after.extend(varint_field(14, 1));
        after.extend(tail);
    } else {
        after = before.clone();
    }
    (len_field(5, &before), len_field(5, &after))
}

/// A small model with every kind of field the encoder has, plus a fixed
/// 64-bit and 32-bit field and a field after the tensor's data location.
fn model() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let big: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
    let edge = vec![7u8; MIN_EXTERNAL_BYTES as usize];
    let small = vec![1u8; MIN_EXTERNAL_BYTES as usize - 1];
    let node = len_field(1, &len_field(1, b"input_ids"));
    let (big_before, big_after) = tensor("embeddings", &big, 0, &[]);
    let (small_before, small_after) = tensor("bias", &small, 0, &[]);
    // A metadata entry (field 16) after the data: the new fields go before it.
    let tail = len_field(16, &len_field(1, b"k"));
    let (edge_before, edge_after) = tensor("edge", &edge, big.len() as u64, &tail);
    // A float_data tensor (field 4) has no raw data and stays.
    let floats = len_field(5, &[varint_field(2, 1), len_field(4, &[0; 8])].concat());
    let graph = |tensors: [&[u8]; 3]| {
        let mut graph = node.clone();
        graph.extend(len_field(2, b"main"));
        graph.extend(tensors[0]);
        graph.extend(tensors[1]);
        graph.extend(&floats);
        graph.extend(tensors[2]);
        graph.extend(len_field(11, &len_field(1, b"input_ids")));
        graph
    };
    let model = |graph: Vec<u8>| {
        let mut model = varint_field(1, 8); // ir_version
        model.extend(len_field(2, b"pytorch"));
        model.extend([0x29, 1, 2, 3, 4, 5, 6, 7, 8]); // a fixed64 field 5
        model.extend(len_field(7, &graph));
        model.extend([0x75, 1, 2, 3, 4]); // a fixed32 field 14
        model.extend(len_field(8, &len_field(2, &varint_field(1, 17))));
        model
    };
    let before = model(graph([&big_before, &small_before, &edge_before]));
    let after = model(graph([&big_after, &small_after, &edge_after]));
    (before, after, [big, edge].concat())
}

fn folder_names(folder: &Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn weights_move_beside_the_model_in_graph_order_and_everything_else_is_kept() {
    let (before, after, data) = model();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("encoder.onnx");
    std::fs::write(&path, &before).unwrap();
    let pins = ExternalData {
        model: pin(&after),
        data: pin(&data),
    };
    convert(&path, &pin(&before), &pins, &|| Ok(())).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), after);
    assert_eq!(std::fs::read(data_path(&path)).unwrap(), data);
    assert_eq!(
        folder_names(dir.path()),
        ["encoder.onnx", "encoder.onnx.data"]
    );
    assert!(present(&path, &pins));
    verify(&path, &pins).unwrap();
}

/// A wrong input or output digest, or a cancel, leaves the upstream file
/// alone and no temporary file behind.
#[test]
fn a_failed_or_cancelled_rewrite_leaves_the_upstream_layout() {
    let (before, after, data) = model();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("encoder.onnx");
    std::fs::write(&path, &before).unwrap();
    let pins = ExternalData {
        model: pin(&after),
        data: pin(&data),
    };
    let mut wrong_model = pins.clone();
    wrong_model.model.sha256 = sha(b"other");
    let mut wrong_data = pins.clone();
    wrong_data.data.bytes += 1;
    let mut wrong_input = pin(&before);
    wrong_input.sha256 = sha(b"other");
    for (upstream, pins) in [
        (pin(&before), &wrong_model),
        (pin(&before), &wrong_data),
        (wrong_input, &pins),
    ] {
        let error = convert(&path, &upstream, pins, &|| Ok(())).unwrap_err();
        assert!(
            matches!(error, DetectError::Download(DownloadError::Integrity)),
            "{error:?}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(folder_names(dir.path()), ["encoder.onnx"]);
    }
    // A cancel at each of the rewrite's checks in turn.
    let mut cancelled = 0;
    loop {
        let checks = AtomicUsize::new(0);
        let result = convert(&path, &pin(&before), &pins, &|| {
            if checks.fetch_add(1, Ordering::Relaxed) == cancelled {
                Err(DetectError::Cancelled)
            } else {
                Ok(())
            }
        });
        if result.is_ok() {
            break;
        }
        assert!(matches!(result, Err(DetectError::Cancelled)), "{result:?}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(folder_names(dir.path()), ["encoder.onnx"]);
        cancelled += 1;
    }
    assert!(cancelled >= 3, "only {cancelled} checks");
    std::fs::write(&path, &before).unwrap();
    // A stale data file from an interrupted rewrite is replaced.
    std::fs::write(data_path(&path), b"stale").unwrap();
    convert(&path, &pin(&before), &pins, &|| Ok(())).unwrap();
    assert_eq!(std::fs::read(data_path(&path)).unwrap(), data);
}

#[test]
fn a_truncated_or_unexpected_model_is_refused() {
    let (before, after, data) = model();
    let pins = ExternalData {
        model: pin(&after),
        data: pin(&data),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("encoder.onnx");
    // Cut inside the graph; a model with nothing to move; a wrong size.
    let cut = &before[..before.len() / 2];
    let nothing = varint_field(1, 8);
    for (bytes, upstream) in [(cut, pin(cut)), (&nothing[..], pin(&nothing))] {
        std::fs::write(&path, bytes).unwrap();
        assert!(matches!(
            convert(&path, &upstream, &pins, &|| Ok(())),
            Err(DetectError::Download(DownloadError::Integrity))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    // A zero-filled file of the pinned size fails its digest before it is
    // parsed; one that matched its pin would still be refused at once.
    let zeros = vec![0u8; before.len()];
    std::fs::write(&path, &zeros).unwrap();
    for upstream in [pin(&before), pin(&zeros)] {
        assert!(matches!(
            convert(&path, &upstream, &pins, &|| Ok(())),
            Err(DetectError::Download(DownloadError::Integrity))
        ));
    }
    std::fs::write(&path, &before).unwrap();
    let mut short = pin(&before);
    short.bytes -= 1;
    assert!(matches!(
        convert(&path, &short, &pins, &|| Ok(())),
        Err(DetectError::Download(DownloadError::SizeMismatch))
    ));
    assert_eq!(folder_names(dir.path()), ["encoder.onnx"]);
}

/// The pinned encoders convert to exactly their pins, which Python's onnx
/// writes too (#905). Reads a copy, never the profile:
/// `MDOC_UPSTREAM_ENCODER=<folder with a copy of the pinned fp16_v2 or
/// fp32_v2 encoder> cargo nextest run -p mdoc-detect --run-ignored all
/// pinned_encoders`. The copy is replaced by the converted pair.
#[test]
#[ignore = "requires a copy of a pinned upstream encoder; no downloads"]
fn pinned_encoders_convert_to_their_pins() {
    let folder = PathBuf::from(std::env::var_os("MDOC_UPSTREAM_ENCODER").unwrap());
    let mut converted = 0;
    for model in mdoc_store::settings::PiiModel::ALL {
        for artifact in crate::manifest_for(model).files {
            let Some(pins) = &artifact.external_data else {
                continue;
            };
            let path = folder.join(Path::new(&artifact.path).file_name().unwrap());
            if !path.exists() {
                continue;
            }
            let started = std::time::Instant::now();
            convert(&path, &artifact.pin(), pins, &|| Ok(())).unwrap();
            verify(&path, pins).unwrap();
            eprintln!("{}: {:.1}s", artifact.path, started.elapsed().as_secs_f64());
            converted += 1;
        }
    }
    assert!(converted > 0, "no pinned encoder in {}", folder.display());
}
