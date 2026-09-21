//! Lazy local-image decoding, scoped to the currently open Markdown file.
use gpui::{App, Entity, RenderImage};
use mdoc_editor::EditorState;
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, sync::Arc};

#[derive(Default)]
struct Cache {
    images: HashMap<String, Option<Arc<RenderImage>>>,
    released: bool,
}

#[derive(Default)]
pub struct ImageCache(Rc<RefCell<Cache>>);
impl ImageCache {
    pub fn release(&mut self, window: &mut gpui::Window, cx: &mut App) {
        let mut cache = self.0.borrow_mut();
        cache.released = true;
        for image in std::mem::take(&mut cache.images).into_values().flatten() {
            cx.drop_image(image, Some(window));
        }
    }
}

pub fn install(editor: &Entity<EditorState>, directory: PathBuf, cx: &mut App) -> ImageCache {
    let handle = ImageCache::default();
    let cache = handle.0.clone();
    let weak = editor.downgrade();
    let async_cx = cx.to_async();
    editor.update(cx, |editor, _| {
        editor.set_block_image_provider(move |src| {
            if let Some(image) = cache.borrow().images.get(src) {
                return image.clone();
            }
            if cache.borrow().released {
                return None;
            }
            let path = crate::document::local_path(src, &directory)?;
            cache.borrow_mut().images.insert(src.to_owned(), None);
            let cache = cache.clone();
            let key = src.to_owned();
            let weak = weak.clone();
            async_cx
                .spawn(async move |cx| {
                    let image = cx
                        .background_executor()
                        .spawn(async move {
                            let reader = image::ImageReader::open(path)
                                .ok()?
                                .with_guessed_format()
                                .ok()?;
                            let mut decoded =
                                reader.decode().ok()?.thumbnail(2048, 2048).into_rgba8();
                            for pixel in decoded.pixels_mut() {
                                pixel.0.swap(0, 2);
                            }
                            Some(Arc::new(RenderImage::new(vec![image::Frame::new(decoded)])))
                        })
                        .await;
                    if cache.borrow().released {
                        return;
                    }
                    cache.borrow_mut().images.insert(key, image);
                    let _ = weak.update(cx, |_, cx| cx.notify());
                })
                .detach();
            None
        });
    });
    handle
}
