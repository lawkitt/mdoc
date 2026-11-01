//! mdoc patch (ADR 0084): one `NSStatusItem` in the menu bar, whose button
//! calls back on click, and the button's place on screen, so the app can
//! anchor a `WindowKind::PopUp` window under it.

use crate::ns_string;
use cocoa::{
    base::{id, nil},
    foundation::{NSData, NSRect, NSSize},
};
use gpui::{Bounds, DisplayId, Pixels, point, px, size};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel, YES},
    sel, sel_impl,
};
use std::{cell::RefCell, ffi::c_void, rc::Rc, sync::Once};

/// `NSSquareStatusItemLength`.
const SQUARE_LENGTH: f64 = -2.;

/// The menu bar item: its glyph, drawn as a template image (monochrome, no
/// badge), its accessibility label, and what a click does. The callback runs
/// inside an AppKit event; it should only queue work for the app.
pub struct StatusItem {
    pub image: StatusImage,
    pub label: &'static str,
    pub on_click: Box<dyn Fn()>,
}

/// A square glyph `points` wide, as PNGs at one or more scales (say 1x and
/// 2x), black on transparent. Only the shape counts: AppKit tints a template
/// image to suit the menu bar.
pub struct StatusImage {
    pub points: f64,
    pub pngs: &'static [&'static [u8]],
}

struct Installed {
    item: id,
    target: id,
    on_click: Rc<dyn Fn()>,
}

thread_local! {
    static INSTALLED: RefCell<Option<Installed>> = const { RefCell::new(None) };
}

static REGISTER: Once = Once::new();
static mut TARGET_CLASS: *const Class = std::ptr::null();

extern "C" fn clicked(_: &Object, _: Sel, _: id) {
    // Cloned out first: the callback may replace the item.
    let on_click = INSTALLED.with(|installed| {
        installed
            .borrow()
            .as_ref()
            .map(|installed| installed.on_click.clone())
    });
    if let Some(on_click) = on_click {
        on_click();
    }
}

fn target_class() -> *const Class {
    REGISTER.call_once(|| {
        let mut decl = ClassDecl::new("MDOCStatusItemTarget", class!(NSObject)).unwrap();
        // SAFETY: the method's signature matches the `clicked:` selector.
        unsafe {
            decl.add_method(sel!(clicked:), clicked as extern "C" fn(&Object, Sel, id));
            TARGET_CLASS = decl.register();
        }
    });
    // SAFETY: written once, inside `call_once`, before this read.
    unsafe { TARGET_CLASS }
}

/// Show `item` in the menu bar, replacing one shown before, or remove it
/// with `None`. Main thread only.
pub fn set_status_item(item: Option<StatusItem>) {
    remove();
    let Some(item) = item else {
        return;
    };
    // SAFETY: AppKit calls on the main thread; nil results are checked and
    // the item and target are retained until `remove`.
    unsafe {
        let bar: id = msg_send![class!(NSStatusBar), systemStatusBar];
        let status: id = msg_send![bar, statusItemWithLength: SQUARE_LENGTH];
        if status.is_null() {
            return;
        }
        let _: id = msg_send![status, retain];
        let button: id = msg_send![status, button];
        let image = template_image(&item.image, item.label);
        if image.is_null() {
            let _: () = msg_send![button, setTitle: ns_string("mdoc")];
        } else {
            let _: () = msg_send![button, setImage: image];
        }
        let _: () = msg_send![button, setToolTip: ns_string(item.label)];
        let target: id = msg_send![target_class(), new];
        let _: () = msg_send![button, setTarget: target];
        let _: () = msg_send![button, setAction: sel!(clicked:)];
        INSTALLED.with(|installed| {
            *installed.borrow_mut() = Some(Installed {
                item: status,
                target,
                on_click: item.on_click.into(),
            })
        });
    }
}

/// An autoreleased template `NSImage` with one representation per PNG that
/// decodes, each `points` square; nil when none does.
unsafe fn template_image(glyph: &StatusImage, label: &str) -> id {
    unsafe {
        let size = NSSize::new(glyph.points, glyph.points);
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithSize: size];
        let mut added = false;
        for png in glyph.pngs {
            let data = NSData::dataWithBytes_length_(nil, png.as_ptr() as *const c_void, png.len() as u64);
            let rep: id = msg_send![class!(NSBitmapImageRep), imageRepWithData: data];
            if rep.is_null() {
                continue;
            }
            let _: () = msg_send![rep, setSize: size];
            let _: () = msg_send![image, addRepresentation: rep];
            added = true;
        }
        if !added {
            let _: () = msg_send![image, release];
            return nil;
        }
        let _: () = msg_send![image, setTemplate: YES];
        let _: () = msg_send![image, setAccessibilityDescription: ns_string(label)];
        let _: id = msg_send![image, autorelease];
        image
    }
}

fn remove() {
    let Some(installed) = INSTALLED.with(|installed| installed.borrow_mut().take()) else {
        return;
    };
    // SAFETY: both objects were retained by `set_status_item`.
    unsafe {
        let bar: id = msg_send![class!(NSStatusBar), systemStatusBar];
        let _: () = msg_send![bar, removeStatusItem: installed.item];
        let _: () = msg_send![installed.item, release];
        let _: () = msg_send![installed.target, release];
    }
}

/// The status item's button: its display and its bounds on that display,
/// top-left origin, as `WindowOptions` takes them. `None` without an item.
pub fn status_item_bounds() -> Option<(DisplayId, Bounds<Pixels>)> {
    let item = INSTALLED.with(|installed| installed.borrow().as_ref().map(|i| i.item))?;
    // SAFETY: main thread; the item is retained while installed and every
    // nil result is checked.
    unsafe {
        let button: id = msg_send![item, button];
        let window: id = msg_send![button, window];
        if window.is_null() {
            return None;
        }
        let frame: NSRect = msg_send![window, frame];
        let mut screen: id = msg_send![window, screen];
        if screen.is_null() {
            screen = msg_send![class!(NSScreen), mainScreen];
        }
        if screen.is_null() {
            return None;
        }
        let screen_frame: NSRect = msg_send![screen, frame];
        let display = crate::window::display_id_for_screen(screen)?;
        let top =
            screen_frame.origin.y + screen_frame.size.height - frame.origin.y - frame.size.height;
        Some((
            DisplayId::new(display as u64),
            Bounds {
                origin: point(
                    px((frame.origin.x - screen_frame.origin.x) as f32),
                    px(top as f32),
                ),
                size: size(px(frame.size.width as f32), px(frame.size.height as f32)),
            },
        ))
    }
}
