//! The encoder re-saved with its weights in a file beside it (ONNX external
//! data). ONNX Runtime keeps a second copy of weights stored inside an
//! `.onnx` file, so the pinned upstream encoder peaks about twice as high as
//! the same weights loaded from a `<name>.onnx.data` file (#905).
//!
//! The rewrite is a re-serialization with the same bytes: every initializer
//! of the main graph whose `raw_data` is at least [`MIN_EXTERNAL_BYTES`]
//! moves, in graph order and without padding, to the data file, and its
//! `TensorProto` gets `external_data` (`location`, `offset`, `length`) and
//! `data_location = EXTERNAL` in place of `raw_data`. Everything else is
//! copied byte for byte. This is what Python's
//! `onnx.save_model(m, out, save_as_external_data=True,
//! all_tensors_to_one_file=True, location="<name>.onnx.data",
//! size_threshold=1024)` writes for these models, so the pinned digests can
//! be reproduced with it.
//!
//! The input streams through twice and is never held in memory: one pass
//! reads the protobuf structure, skipping weight payloads, and one copies
//! bytes to the two outputs while hashing all three files. Outputs land in
//! temporary files beside the encoder and replace it only when the input and
//! both outputs match their pinned digests.
use crate::DetectError;
use mdoc_import::model_download::DownloadError;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
};

/// The smallest `raw_data` moved out of the model file. Python's onnx
/// compares `sys.getsizeof(raw_data)`, 33 bytes over the length, with its
/// `size_threshold` of 1024.
pub const MIN_EXTERNAL_BYTES: u64 = 1024 - 33;

/// One output file's pin.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Pin {
    pub bytes: u64,
    pub sha256: String,
}

/// The pinned result of re-saving an artifact with external data: the
/// model file at the artifact's path and the weights at `<path>.data`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ExternalData {
    pub model: Pin,
    pub data: Pin,
}

/// Where the data file goes: beside the model, its name plus `.data`.
pub fn data_path(model: &Path) -> PathBuf {
    let mut name = model.file_name().unwrap_or_default().to_os_string();
    name.push(".data");
    model.with_file_name(name)
}

/// Whether both converted files are in place at their pinned sizes.
pub fn present(model: &Path, pins: &ExternalData) -> bool {
    !mdoc_import::model_download::missing(model, pins.model.bytes)
        && !mdoc_import::model_download::missing(&data_path(model), pins.data.bytes)
}

/// Check both converted files against their pins.
pub fn verify(model: &Path, pins: &ExternalData) -> Result<(), DownloadError> {
    mdoc_import::model_download::verify(model, pins.model.bytes, &pins.model.sha256)?;
    mdoc_import::model_download::verify(&data_path(model), pins.data.bytes, &pins.data.sha256)
}

/// Re-save the upstream file at `model` (pinned as `upstream`) with
/// external data, replacing it in place. A crash or error at any point
/// leaves a loadable layout: the data file is renamed in first, and the
/// model last, over the upstream file.
pub fn convert(
    model: &Path,
    upstream: &Pin,
    pins: &ExternalData,
    check: &dyn Fn() -> Result<(), DetectError>,
) -> Result<(), DetectError> {
    let location = data_path(model)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(DownloadError::InvalidPath)?
        .to_owned();
    let folder = model.parent().ok_or(DownloadError::InvalidPath)?;
    let plan = {
        let mut input = Input::new(File::open(model)?)?;
        if input.len != upstream.bytes {
            return Err(DownloadError::SizeMismatch.into());
        }
        plan(&mut input, &location)?
    };
    check()?;
    let mut model_out = Hashed::new(tempfile::NamedTempFile::new_in(folder)?);
    let mut data_out = Hashed::new(tempfile::NamedTempFile::new_in(folder)?);
    let mut input = Hashed::new(BufReader::with_capacity(1 << 20, File::open(model)?));
    let mut buffer = vec![0; 1 << 20];
    let mut copied = 0u64;
    for op in &plan {
        let (len, out): (u64, Option<&mut dyn Write>) = match op {
            Op::Emit(bytes) => {
                model_out.write_all(bytes)?;
                continue;
            }
            Op::Model(len) => (*len, Some(&mut model_out)),
            Op::Data(len) => (*len, Some(&mut data_out)),
            Op::Skip(len) => (*len, None),
        };
        let mut left = len;
        let mut out = out;
        while left > 0 {
            let n = left.min(buffer.len() as u64) as usize;
            input.read_exact(&mut buffer[..n])?;
            if let Some(out) = out.as_deref_mut() {
                out.write_all(&buffer[..n])?;
            }
            left -= n as u64;
            copied += n as u64;
            if copied >= 64 << 20 {
                copied = 0;
                check()?;
            }
        }
    }
    // The plan covers the whole input; anything after it is a damaged file.
    if input.inner.read(&mut buffer[..1])? != 0 {
        return Err(DownloadError::Integrity.into());
    }
    let matches =
        |hashed: (u64, String), pin: &Pin| hashed.0 == pin.bytes && hashed.1 == pin.sha256;
    if !matches(input.finish(), upstream)
        || !matches(model_out.finish(), &pins.model)
        || !matches(data_out.finish(), &pins.data)
    {
        return Err(DownloadError::Integrity.into());
    }
    // Windows refuses to replace a file that is still open.
    drop(input);
    check()?;
    let persist = |file: tempfile::NamedTempFile, to: &Path| -> Result<(), DetectError> {
        file.as_file().sync_all()?;
        file.persist(to)
            .map_err(|e| DownloadError::Persist(std::sync::Arc::new(e)))?;
        Ok(())
    };
    persist(data_out.inner, &data_path(model))?;
    persist(model_out.inner, model)?;
    #[cfg(unix)]
    File::open(folder)?.sync_all()?;
    Ok(())
}

/// A writer or reader that counts and hashes what passes through it.
struct Hashed<T> {
    inner: T,
    digest: Sha256,
    bytes: u64,
}

impl<T> Hashed<T> {
    fn new(inner: T) -> Self {
        Self {
            inner,
            digest: Sha256::new(),
            bytes: 0,
        }
    }

    fn finish(&mut self) -> (u64, String) {
        let digest = std::mem::take(&mut self.digest).finalize();
        (
            self.bytes,
            digest.iter().map(|b| format!("{b:02x}")).collect(),
        )
    }
}

impl Write for Hashed<tempfile::NamedTempFile> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(bytes)?;
        self.digest.update(&bytes[..n]);
        self.bytes += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<R: Read> Read for Hashed<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buffer)?;
        self.digest.update(&buffer[..n]);
        self.bytes += n as u64;
        Ok(n)
    }
}

/// One step of the copy, in input order: input bytes to the model file, to
/// the data file or dropped, or new bytes for the model file.
#[derive(Debug, PartialEq, Eq)]
enum Op {
    Model(u64),
    Data(u64),
    Skip(u64),
    Emit(Vec<u8>),
}

fn damaged() -> DetectError {
    DownloadError::Integrity.into()
}

/// The input's structure, read through with weight payloads skipped.
struct Input {
    reader: BufReader<File>,
    pos: u64,
    len: u64,
}

impl Input {
    fn new(file: File) -> io::Result<Self> {
        let len = file.metadata()?.len();
        Ok(Self {
            reader: BufReader::with_capacity(1 << 16, file),
            pos: 0,
            len,
        })
    }

    fn byte(&mut self) -> Result<u8, DetectError> {
        let mut byte = [0];
        self.reader.read_exact(&mut byte).map_err(|_| damaged())?;
        self.pos += 1;
        Ok(byte[0])
    }

    fn varint(&mut self) -> Result<u64, DetectError> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.byte()?;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(damaged())
    }

    fn skip(&mut self, len: u64) -> Result<(), DetectError> {
        if len > self.len - self.pos {
            return Err(damaged());
        }
        self.reader
            .seek_relative(i64::try_from(len).map_err(|_| damaged())?)?;
        self.pos += len;
        Ok(())
    }

    /// The next field before `end`: its number, wire type, where it starts
    /// and, for length-delimited fields, its payload's length. Other fields
    /// are skipped whole.
    fn field(&mut self, end: u64) -> Result<Option<Field>, DetectError> {
        if self.pos == end {
            return Ok(None);
        }
        let start = self.pos;
        let tag = self.varint()?;
        let (number, wire) = (tag >> 3, tag & 7);
        let len = match wire {
            0 => {
                self.varint()?;
                None
            }
            1 => {
                self.skip(8)?;
                None
            }
            2 => Some(self.varint()?),
            5 => {
                self.skip(4)?;
                None
            }
            _ => return Err(damaged()),
        };
        let header = self.pos - start;
        if len.is_some_and(|len| len > end.saturating_sub(self.pos)) || self.pos > end {
            return Err(damaged());
        }
        Ok(Some(Field {
            number,
            start,
            header,
            len,
        }))
    }
}

struct Field {
    number: u64,
    start: u64,
    /// The tag and length prefix's bytes.
    header: u64,
    /// A length-delimited field's payload length.
    len: Option<u64>,
}

impl Field {
    fn size(&self) -> u64 {
        self.header + self.len.unwrap_or(0)
    }
}

// ModelProto.graph, GraphProto.initializer, and TensorProto's fields.
const MODEL_GRAPH: u64 = 7;
const GRAPH_INITIALIZER: u64 = 5;
const TENSOR_RAW_DATA: u64 = 9;
const TENSOR_EXTERNAL_DATA: u64 = 13;
const TENSOR_DATA_LOCATION: u64 = 14;
const LOCATION_EXTERNAL: u64 = 1;

fn plan(input: &mut Input, location: &str) -> Result<Vec<Op>, DetectError> {
    let mut ops = Vec::new();
    let mut offset = 0;
    while let Some(field) = input.field(input.len)? {
        match field.len {
            Some(_) if field.number == MODEL_GRAPH => {
                let (graph, size) =
                    plan_graph(input, field.start + field.size(), location, &mut offset)?;
                ops.push(Op::Skip(field.header));
                ops.push(Op::Emit(header(MODEL_GRAPH, size)));
                ops.extend(graph);
            }
            Some(len) => {
                input.skip(len)?;
                ops.push(Op::Model(field.size()));
            }
            None => ops.push(Op::Model(field.size())),
        }
    }
    if offset == 0 {
        // Nothing to move: not the export setup pinned.
        return Err(damaged());
    }
    Ok(merge(ops))
}

/// The graph's ops and its new payload size.
fn plan_graph(
    input: &mut Input,
    end: u64,
    location: &str,
    offset: &mut u64,
) -> Result<(Vec<Op>, u64), DetectError> {
    let mut ops = Vec::new();
    let mut size = 0;
    while let Some(field) = input.field(end)? {
        match field.len {
            Some(_) if field.number == GRAPH_INITIALIZER => {
                let (tensor, tensor_size) =
                    plan_tensor(input, field.start + field.size(), location, offset)?;
                match tensor_size {
                    None => {
                        ops.push(Op::Model(field.size()));
                        size += field.size();
                    }
                    Some(tensor_size) => {
                        let header = header(GRAPH_INITIALIZER, tensor_size);
                        size += header.len() as u64 + tensor_size;
                        ops.push(Op::Skip(field.header));
                        ops.push(Op::Emit(header));
                        ops.extend(tensor);
                    }
                }
            }
            Some(len) => {
                input.skip(len)?;
                ops.push(Op::Model(field.size()));
                size += field.size();
            }
            None => {
                ops.push(Op::Model(field.size()));
                size += field.size();
            }
        }
    }
    Ok((ops, size))
}

/// A tensor's ops and, when its weights move, its new payload size. Fields
/// keep their order; `external_data` and `data_location` go where field
/// order puts them, after every field numbered below them.
fn plan_tensor(
    input: &mut Input,
    end: u64,
    location: &str,
    offset: &mut u64,
) -> Result<(Vec<Op>, Option<u64>), DetectError> {
    let mut fields = Vec::new();
    while let Some(field) = input.field(end)? {
        if let Some(len) = field.len {
            input.skip(len)?;
        }
        fields.push(field);
    }
    let unchanged = |fields: &[Field]| {
        let size = fields.iter().map(Field::size).sum();
        Ok((vec![Op::Model(size)], None))
    };
    let raw: Vec<&Field> = fields
        .iter()
        .filter(|f| f.number == TENSOR_RAW_DATA)
        .collect();
    let already = fields
        .iter()
        .any(|f| matches!(f.number, TENSOR_EXTERNAL_DATA | TENSOR_DATA_LOCATION));
    let [raw] = raw[..] else {
        return unchanged(&fields);
    };
    let Some(length) = raw.len.filter(|len| *len >= MIN_EXTERNAL_BYTES) else {
        return unchanged(&fields);
    };
    if already {
        return Err(damaged());
    }
    let mut added = Vec::new();
    for (key, value) in [
        ("location", location.to_owned()),
        ("offset", offset.to_string()),
        ("length", length.to_string()),
    ] {
        let mut entry = header(1, key.len() as u64);
        entry.extend(key.as_bytes());
        entry.extend(header(2, value.len() as u64));
        entry.extend(value.as_bytes());
        added.extend(header(TENSOR_EXTERNAL_DATA, entry.len() as u64));
        added.extend(entry);
    }
    added.extend(varint(TENSOR_DATA_LOCATION << 3));
    added.extend(varint(LOCATION_EXTERNAL));
    *offset += length;
    let mut ops = Vec::new();
    let mut size = added.len() as u64;
    let mut added = Some(added);
    for field in &fields {
        if field.number > TENSOR_DATA_LOCATION {
            if let Some(added) = added.take() {
                ops.push(Op::Emit(added));
            }
        } else if added.is_none() {
            // Out of field order: a field below 13 after one above 14.
            return Err(damaged());
        }
        if field.number == TENSOR_RAW_DATA {
            ops.push(Op::Skip(field.header));
            ops.push(Op::Data(length));
        } else {
            ops.push(Op::Model(field.size()));
            size += field.size();
        }
    }
    if let Some(added) = added {
        ops.push(Op::Emit(added));
    }
    Ok((ops, Some(size)))
}

/// Adjacent copies of one kind as one.
fn merge(ops: Vec<Op>) -> Vec<Op> {
    let mut merged: Vec<Op> = Vec::with_capacity(ops.len());
    for op in ops {
        match (merged.last_mut(), op) {
            (Some(Op::Model(a)), Op::Model(b)) | (Some(Op::Skip(a)), Op::Skip(b)) => *a += b,
            (Some(Op::Emit(a)), Op::Emit(b)) => a.extend(b),
            (_, op) => merged.push(op),
        }
    }
    merged
}

fn varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            bytes.push(byte);
            return bytes;
        }
        bytes.push(byte | 0x80);
    }
}

/// A length-delimited field's tag and length.
fn header(number: u64, len: u64) -> Vec<u8> {
    let mut bytes = varint(number << 3 | 2);
    bytes.extend(varint(len));
    bytes
}

#[cfg(test)]
mod tests;
