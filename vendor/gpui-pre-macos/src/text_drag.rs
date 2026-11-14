// mdoc patch (ADR 0084): included into window.rs, so it shares that module's
// window state. A drag of one pasteboard item carrying both plain text and a
// file URL, for the menu bar shelf.

/// mdoc patch (ADR 0084): the card a shelf drag shows under the pointer:
/// the document's title and first line, in the shelf's light or dark look.
pub struct DragCard<'a> {
    pub title: &'a str,
    pub line: &'a str,
    pub dark: bool,
}

/// The drag card's size in points.
const DRAG_CARD_WIDTH: f64 = 240.;
const DRAG_CARD_HEIGHT: f64 = 52.;

/// mdoc patch (ADR 0084): the running text-and-file drag's end callback and
/// its source window, retained until the drag ends so AppKit can still
/// report the end after the window closes.
struct TextDragEnd {
    on_end: Box<dyn FnOnce(bool)>,
    _source: Retained<AnyObject>,
    /// The card or icon the drag shows, kept for its image provider.
    _image: Retained<AnyObject>,
}

thread_local! {
    static TEXT_DRAG_END: std::cell::RefCell<Option<TextDragEnd>> =
        const { std::cell::RefCell::new(None) };
}

/// End the running text-and-file drag, if any: its callback runs once with
/// whether something took the drop, and its source window is released.
fn finish_text_and_file_drag(dropped: bool) {
    let Some(end) = TEXT_DRAG_END.with(|end| end.borrow_mut().take()) else {
        return;
    };
    let TextDragEnd { on_end, .. } = end;
    on_end(dropped);
}

/// The card as an image: a rounded panel with the title in semibold and the
/// line in muted text, each cut with "…".
unsafe fn drag_card_image(card: &DragCard<'_>) -> Option<Retained<AnyObject>> {
    unsafe {
        let size = NSSize::new(DRAG_CARD_WIDTH, DRAG_CARD_HEIGHT);
        let image: *mut AnyObject = msg_send![class!(NSImage), alloc];
        let image: *mut AnyObject = msg_send![image, initWithSize: size];
        let image = Retained::from_raw(image)?;
        let color = |gray: f64, alpha: f64| -> *mut AnyObject {
            msg_send![
                class!(NSColor),
                colorWithSRGBRed: gray,
                green: gray,
                blue: gray,
                alpha: alpha
            ]
        };
        let (fill, border, title_color, line_color) = if card.dark {
            (color(0.16, 0.96), color(1., 0.16), color(0.95, 1.), color(0.68, 1.))
        } else {
            (color(0.99, 0.96), color(0., 0.14), color(0.1, 1.), color(0.4, 1.))
        };
        let _: () = msg_send![&*image, lockFocus];
        let bounds = NSRect::new(
            NSPoint::new(0.5, 0.5),
            NSSize::new(size.width - 1., size.height - 1.),
        );
        let path: *mut AnyObject = msg_send![
            class!(NSBezierPath),
            bezierPathWithRoundedRect: bounds,
            xRadius: 8f64,
            yRadius: 8f64
        ];
        let _: () = msg_send![fill, setFill];
        let _: () = msg_send![path, fill];
        let _: () = msg_send![border, setStroke];
        let _: () = msg_send![path, stroke];
        let style: Option<Retained<AnyObject>> =
            msg_send![class!(NSMutableParagraphStyle), new];
        let style = style?;
        // NSLineBreakByTruncatingTail
        let _: () = msg_send![&*style, setLineBreakMode: 4 as NSUInteger];
        let style_ptr = Retained::as_ptr(&style).cast_mut();
        let draw = |text: &str, font: *mut AnyObject, color: *mut AnyObject, y: f64| {
            let keys = [
                ns_string("NSFont"),
                ns_string("NSColor"),
                ns_string("NSParagraphStyle"),
            ];
            let values = [font, color, style_ptr];
            let attributes: *mut AnyObject = msg_send![
                class!(NSDictionary),
                dictionaryWithObjects: values.as_ptr(),
                forKeys: keys.as_ptr(),
                count: 3 as NSUInteger
            ];
            let rect = NSRect::new(NSPoint::new(12., y), NSSize::new(size.width - 24., 17.));
            let _: () = msg_send![ns_string(text), drawInRect: rect, withAttributes: attributes];
        };
        let title_font: *mut AnyObject = msg_send![class!(NSFont), boldSystemFontOfSize: 12f64];
        let line_font: *mut AnyObject = msg_send![class!(NSFont), systemFontOfSize: 11f64];
        draw(card.title, title_font, title_color, 27.);
        draw(card.line, line_font, line_color, 9.);
        let _: () = msg_send![&*image, unlockFocus];
        Some(image)
    }
}

/// mdoc patch (ADR 0084): start an outbound drag from `window` carrying one
/// pasteboard item with both `text` (plain text) and `file`'s URL, so a chat
/// box takes the text and Finder takes the file. Starts from the window's
/// last left mouse down, like `start_external_drag`. The pointer carries
/// `card` (or the `.md` icon without one). `on_end` runs exactly once, on
/// the main thread: with whether something took the drop when the drag
/// ends (cancelled included), or with `false` at once if it cannot start.
/// Returns whether AppKit started the drag.
pub fn start_text_and_file_drag(
    window: &gpui::Window,
    text: &str,
    file: &std::path::Path,
    card: Option<DragCard<'_>>,
    on_end: Box<dyn FnOnce(bool)>,
) -> bool {
    // A drag that never reported its end ends now, so each callback runs once.
    finish_text_and_file_drag(false);
    match begin_text_and_file_drag(window, text, file, card) {
        Some((source, image)) => {
            TEXT_DRAG_END.with(|end| {
                *end.borrow_mut() = Some(TextDragEnd {
                    on_end,
                    _source: source,
                    _image: image,
                })
            });
            true
        }
        None => {
            on_end(false);
            false
        }
    }
}

/// Begin the drag; when AppKit started it, the source window and the image
/// the drag shows, both retained.
fn begin_text_and_file_drag(
    window: &gpui::Window,
    text: &str,
    file: &std::path::Path,
    card: Option<DragCard<'_>>,
) -> Option<(Retained<AnyObject>, Retained<AnyObject>)> {
    let Ok(handle) = rwh::HasWindowHandle::window_handle(window) else {
        return None;
    };
    let rwh::RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    let native_view: *mut AnyObject = handle.ns_view.as_ptr().cast();
    // SAFETY: the handle's view is the GPUIView of a live window on the main
    // thread, which holds WINDOW_STATE_IVAR; nil results are checked.
    unsafe {
        let window_state = get_window_state(&*native_view);
        let (native_window, last_left_mouse_down_event) = {
            let state = window_state.lock();
            (state.native_window, state.last_left_mouse_down_event.clone())
        };
        let Some(last_left_mouse_down_event) = last_left_mouse_down_event else {
            log::warn!("start_text_and_file_drag declined: no retained left mouse down event");
            return None;
        };
        let event: *mut AnyObject = Retained::as_ptr(&last_left_mouse_down_event)
            .cast_mut()
            .cast();
        let Ok(path_bytes) = CString::new(file.as_os_str().as_bytes()) else {
            return None;
        };
        let url: *mut AnyObject = msg_send![
            class!(NSURL),
            fileURLWithFileSystemRepresentation: path_bytes.as_ptr(),
            isDirectory: Bool::NO,
            relativeToURL: ptr::null_mut::<AnyObject>()
        ];
        if url.is_null() {
            return None;
        }
        let item: *mut AnyObject = msg_send![class!(NSPasteboardItem), alloc];
        let item: *mut AnyObject = msg_send![item, init];
        let item = Retained::from_raw(item)?;
        let url_string: *mut AnyObject = msg_send![url, absoluteString];
        let plain_text = ns_string("public.utf8-plain-text");
        let file_url = ns_string("public.file-url");
        let _: Bool = msg_send![&*item, setString: ns_string(text), forType: plain_text];
        let _: Bool = msg_send![&*item, setString: url_string, forType: file_url];
        let dragging_item: *mut AnyObject = msg_send![class!(NSDraggingItem), alloc];
        let dragging_item: *mut AnyObject =
            msg_send![dragging_item, initWithPasteboardWriter: &*item];
        let dragging_item = Retained::from_raw(dragging_item)?;
        let location: NSPoint = msg_send![event, locationInWindow];
        // The card hangs just below and right of the pointer; the icon
        // centres on it.
        let (contents, size, offset): (Retained<AnyObject>, NSSize, NSPoint) = match &card {
            Some(card) => (
                drag_card_image(card)?,
                NSSize::new(DRAG_CARD_WIDTH, DRAG_CARD_HEIGHT),
                NSPoint::new(-12., -DRAG_CARD_HEIGHT + 8.),
            ),
            None => {
                let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
                let icon: Option<Retained<AnyObject>> =
                    msg_send![workspace, iconForFileType: ns_string("md")];
                (icon?, NSSize::new(32., 32.), NSPoint::new(-16., -16.))
            }
        };
        let frame = NSRect::new(
            NSPoint::new(location.x + offset.x, location.y + offset.y),
            size,
        );
        let provider_contents = contents.clone();
        let provider = RcBlock::new(move || -> *mut AnyObject {
            let component: *mut AnyObject = msg_send![
                class!(NSDraggingImageComponent),
                draggingImageComponentWithKey: NSDraggingImageComponentIconKey
            ];
            let _: () = msg_send![component, setContents: &*provider_contents];
            let _: () = msg_send![
                component,
                setFrame: NSRect::new(NSPoint::new(0., 0.), size)
            ];
            msg_send![class!(NSArray), arrayWithObject: component]
        });
        let _: () = msg_send![&*dragging_item, setDraggingFrame: frame];
        let _: () = msg_send![&*dragging_item, setImageComponentsProvider: &*provider];
        let items: *mut AnyObject = msg_send![class!(NSArray), arrayWithObject: &*dragging_item];
        let session: *mut AnyObject = msg_send![
            native_view,
            beginDraggingSessionWithItems: items,
            event: event,
            source: native_window
        ];
        if session.is_null() {
            return None;
        }
        window_state.lock().synthetic_drag_counter += 1;
        let source = Retained::retain(native_window)?;
        Some((source, contents))
    }
}
