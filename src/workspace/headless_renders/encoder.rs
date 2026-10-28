//! A bounded PNG queue overlaps encoding with subsequent offscreen frames.
use super::{Result, write_png};
use image::RgbaImage;
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{SyncSender, sync_channel},
    thread::{self, JoinHandle},
};

pub(super) struct Encoder {
    input: Option<SyncSender<(PathBuf, RgbaImage)>>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl Encoder {
    pub(super) fn new() -> Result<Self> {
        // At most one queued image, one being encoded, and the render caller's
        // current image. Moving the RGBA buffer avoids the process pipe copy.
        let (input, frames) = sync_channel::<(PathBuf, RgbaImage)>(1);
        let thread = thread::Builder::new()
            .name("headless-png".into())
            .spawn(move || {
                for (path, image) in frames {
                    write_png(&path, &image)?;
                }
                Ok(())
            })
            .map_err(|e| format!("start PNG encoder: {e}"))?;
        Ok(Self {
            input: Some(input),
            thread: Some(thread),
        })
    }

    pub(super) fn write(&mut self, path: &Path, image: RgbaImage) -> Result<()> {
        let sent = self
            .input
            .as_ref()
            .ok_or("PNG encoder already finished")?
            .send((path.to_owned(), image));
        if sent.is_err() {
            // Prefer the actual encode error (or panic) to a disconnected queue.
            self.finish()?;
            return Err("PNG encoder stopped before accepting a frame".into());
        }
        Ok(())
    }

    pub(super) fn finish(&mut self) -> Result<()> {
        drop(self.input.take());
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| "PNG encoder thread panicked".to_string())??;
        }
        Ok(())
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // An earlier render error still waits for the encoder before cleanup.
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_completes_every_queued_png() {
        let out = tempfile::tempdir().unwrap();
        let image = RgbaImage::from_pixel(2, 1, image::Rgba([1, 2, 3, 4]));
        let mut encoder = Encoder::new().unwrap();
        for name in ["first.png", "second.png"] {
            encoder
                .write(&out.path().join(name), image.clone())
                .unwrap();
        }
        encoder.finish().unwrap();
        for name in ["first.png", "second.png"] {
            assert_eq!(
                image::open(out.path().join(name)).unwrap().to_rgba8(),
                image
            );
        }
    }

    #[test]
    fn join_reports_encode_failure() {
        let out = tempfile::tempdir().unwrap();
        let mut encoder = Encoder::new().unwrap();
        encoder
            .write(&out.path().join("missing/image.png"), RgbaImage::new(1, 1))
            .unwrap();
        assert!(encoder.finish().is_err());
    }
}
