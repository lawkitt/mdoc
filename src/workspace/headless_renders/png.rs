//! Two bounded PNG encoders. Rendering and Tab dispatch stay in one process so
//! both themes retain exactly the same stateful scenario sequence.
use super::{Result, write_png};
use image::RgbaImage;
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
};

pub(super) struct Workers {
    children: Vec<Child>,
    root: PathBuf,
    next: usize,
}

impl Workers {
    pub(super) fn new(root: &Path) -> Result<Self> {
        let mut workers = Self {
            children: Vec::new(),
            root: root.to_owned(),
            next: 0,
        };
        for _ in 0..2 {
            workers.children.push(
                Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
                    .arg("--headless-encode-png")
                    .arg(root)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .map_err(|e| format!("start PNG worker: {e}"))?,
            );
        }
        Ok(workers)
    }

    pub(super) fn write(&mut self, path: &Path, image: &RgbaImage) -> Result<()> {
        let name = path
            .strip_prefix(&self.root)
            .map_err(|e| e.to_string())?
            .to_str()
            .ok_or("non-UTF-8 PNG name")?;
        let mut header = [0; 12];
        header[..4].copy_from_slice(&image.width().to_le_bytes());
        header[4..8].copy_from_slice(&image.height().to_le_bytes());
        header[8..].copy_from_slice(&(name.len() as u32).to_le_bytes());
        let input = self.children[self.next]
            .stdin
            .as_mut()
            .ok_or("PNG workers already finished")?;
        // A pipe blocks while its worker encodes the previous image. There is
        // no unbounded frame queue: two worker images plus this caller's image.
        input
            .write_all(&header)
            .and_then(|_| input.write_all(name.as_bytes()))
            .and_then(|_| input.write_all(image.as_raw()))
            .map_err(|e| format!("send PNG {name}: {e}"))?;
        self.next = (self.next + 1) % self.children.len();
        Ok(())
    }

    pub(super) fn finish(&mut self) -> Result<()> {
        for child in &mut self.children {
            drop(child.stdin.take());
        }
        for child in &mut self.children {
            let status = child.wait().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(format!("PNG worker failed: {status}"));
            }
        }
        self.children.clear();
        Ok(())
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn read_frame(input: &mut impl Read) -> Result<Option<(PathBuf, RgbaImage)>> {
    let mut header = [0; 12];
    if input.read(&mut header[..1]).map_err(|e| e.to_string())? == 0 {
        return Ok(None);
    }
    input
        .read_exact(&mut header[1..])
        .map_err(|e| format!("truncated PNG header: {e}"))?;
    let width = u32::from_le_bytes(header[..4].try_into().unwrap());
    let height = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let name_len = u32::from_le_bytes(header[8..].try_into().unwrap()) as usize;
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|&n| n > 0 && n <= 64 * 1024 * 1024)
        .ok_or("invalid PNG dimensions")?;
    if !(1..=4096).contains(&name_len) {
        return Err("invalid PNG name length".into());
    }
    let mut name = vec![0; name_len];
    input.read_exact(&mut name).map_err(|e| e.to_string())?;
    let path = PathBuf::from(String::from_utf8(name).map_err(|e| e.to_string())?);
    if !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err("PNG name must stay under the output directory".into());
    }
    let mut pixels = vec![0; bytes];
    input
        .read_exact(&mut pixels)
        .map_err(|e| format!("truncated PNG pixels: {e}"))?;
    let image = RgbaImage::from_raw(width, height, pixels).ok_or("invalid PNG pixels")?;
    Ok(Some((path, image)))
}

pub(crate) fn run_worker(root: &Path) -> Result<()> {
    // This entry point never constructs app state, a profile, a renderer or a
    // window. EOF also lets workers exit promptly if the render parent dies.
    let mut input = std::io::stdin().lock();
    while let Some((name, image)) = read_frame(&mut input)? {
        write_png(&root.join(name), &image)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(name: &str, pixels: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&(name.len() as u32).to_le_bytes());
        data.extend_from_slice(name.as_bytes());
        data.extend_from_slice(pixels);
        data
    }

    #[test]
    fn frame_stream_distinguishes_eof_from_truncation() {
        assert!(read_frame(&mut &[][..]).unwrap().is_none());
        assert!(read_frame(&mut &[1][..]).unwrap_err().contains("header"));
        let data = frame("screen.png", &[1, 2, 3]);
        assert!(
            read_frame(&mut data.as_slice())
                .unwrap_err()
                .contains("pixels")
        );
        let data = frame("screen/tab-000.png", &[1, 2, 3, 4]);
        let (name, image) = read_frame(&mut data.as_slice()).unwrap().unwrap();
        assert_eq!(name, Path::new("screen/tab-000.png"));
        assert_eq!(image.as_raw(), &[1, 2, 3, 4]);
    }

    #[test]
    fn frame_rejects_escaped_paths_and_oversized_allocations() {
        let data = frame("../outside.png", &[1, 2, 3, 4]);
        assert!(
            read_frame(&mut data.as_slice())
                .unwrap_err()
                .contains("output")
        );
        let mut data = frame("screen.png", &[]);
        data[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(
            read_frame(&mut data.as_slice())
                .unwrap_err()
                .contains("dimensions")
        );
    }
}
