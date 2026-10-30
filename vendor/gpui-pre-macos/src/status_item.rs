//! mdoc patch (ADR 0084): one `NSStatusItem` in the menu bar, whose button
//! calls back on click, and the button's place on screen, so the app can
//! anchor a `WindowKind::PopUp` window under it.

use crate::ns_string;
use cocoa::{base::id, foundation::NSRect};
use gpui::{Bounds, DisplayId, Pixels, point, px, size};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel, YES},
    sel, sel_impl,
};
use std::{cell::RefCell, rc::Rc, sync::Once};

/// `NSSquareStatusItemLength`.
const SQUARE_LENGTH: f64 = -2.;

/// The menu bar item: an SF Symbol drawn as a template image (monochrome, no
/// badge), its accessibility label, and what a click does. The callback runs
/// inside an AppKit event; it should only queue work for the app.
pub struct StatusItem {
    pub symbol: &'static str,
    pub label: &'static str,
    pub on_click: Box<dyn Fn()>,
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
        let image: id = msg_send![
            class!(NSImage),
            imageWithSystemSymbolName: ns_string(item.symbol)
            accessibilityDescription: ns_string(item.label)
        ];
        if image.is_null() {
            let _: () = msg_send![button, setTitle: ns_string("mdoc")];
        } else {
            let _: () = msg_send![image, setTemplate: YES];
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
