//! Tokenizer-bounded model windows, including pathological-word rejection.
use super::*;
pub(super) fn bounded_windows(
    source: &str,
    transformer: &SchemaTransformer,
    tasks: &[SchemaTask],
    cancel: &AtomicBool,
    started: Instant,
) -> Result<Vec<std::ops::Range<usize>>, String> {
    let mut pending: Vec<_> = Chunker::new(128, 32)
        .map_err(|e| e.to_string())?
        .split(source)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|chunk| chunk.byte_start..chunk.byte_end)
        .collect();
    let mut windows = Vec::new();
    while let Some(range) = pending.pop() {
        checkpoint(cancel, started)?;
        let record = transformer
            .transform(&source[range.clone()], tasks)
            .map_err(|e| e.to_string())?;
        if record.input_ids.len() <= MAX_INPUT_TOKENS {
            windows.push(range);
            continue;
        }
        // Subdivide using the model's own word splitter, and verify actual
        // schema+text tokens again. Reject a single pathological word rather
        // than truncate it or silently deliver partial detection coverage.
        let words = record.num_words();
        if words < 2 {
            return Err("A source word exceeds the model token limit. Add selections manually; no partial scan was accepted.".into());
        }
        let size = words.div_ceil(2);
        let overlap = (size / 4).min(size - 1);
        for chunk in Chunker::new(size, overlap)
            .map_err(|e| e.to_string())?
            .split(&source[range.clone()])
            .map_err(|e| e.to_string())?
        {
            pending.push(range.start + chunk.byte_start..range.start + chunk.byte_end);
        }
    }
    windows.sort_by_key(|range| range.start);
    Ok(windows)
}
