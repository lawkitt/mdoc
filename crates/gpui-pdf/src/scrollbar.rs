//! Small overlay scrollbar shared by the PDF viewport and its Markdown host.
use gpui::{
    AnyElement, Context, DispatchPhase, DragMoveEvent, Empty, HitboxBehavior, Hsla, MouseButton,
    Render, ScrollHandle, ScrollWheelEvent, Window, canvas, div, prelude::*, px,
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone)]
struct ThumbDrag {
    grab: Rc<Cell<f32>>,
    owner: gpui::EntityId,
    id: &'static str,
}
impl Render for ThumbDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Metrics {
    start: f32,
    length: f32,
    thumb: f32,
    position: f32,
    max: f32,
}
impl Metrics {
    fn new(start: f32, length: f32, max: f32, offset: f32) -> Option<Self> {
        if length <= 1. || max <= 1. {
            return None;
        }
        let thumb = (length * length / (length + max)).max(32.).min(length);
        let position = (-offset / max).clamp(0., 1.) * (length - thumb);
        Some(Self {
            start,
            length,
            thumb,
            position,
            max,
        })
    }
    fn offset(self, pointer: f32, grab: f32) -> f32 {
        let travel = self.length - self.thumb;
        if travel <= 0. {
            return 0.;
        }
        -((pointer - self.start - grab) / travel).clamp(0., 1.) * self.max
    }
}
fn metrics(scroll: &ScrollHandle, horizontal: bool) -> Option<Metrics> {
    let bounds = scroll.bounds();
    if horizontal {
        Metrics::new(
            bounds.left().into(),
            bounds.size.width.into(),
            scroll.max_offset().x.into(),
            scroll.offset().x.into(),
        )
    } else {
        Metrics::new(
            bounds.top().into(),
            bounds.size.height.into(),
            scroll.max_offset().y.into(),
            scroll.offset().y.into(),
        )
    }
}

/// Draws only when the axis overflows. The other axis is preserved while dragging.
/// Rechecks geometry after layout so the thumb appears on the first content frame.
pub fn overlay_scrollbar<T: 'static>(
    id: &'static str,
    scroll: &ScrollHandle,
    horizontal: bool,
    color: Hsla,
    cx: &mut Context<T>,
) -> AnyElement {
    let current = metrics(scroll, horizontal);
    let measure = scroll.clone();
    let owner = cx.entity().downgrade();
    let wheel_scroll = scroll.clone();
    let mut overlay = div().absolute().top_0().left_0().size_full().child(
        canvas(
            move |bounds, window, _| {
                let hitbox =
                    horizontal.then(|| window.insert_hitbox(bounds, HitboxBehavior::Normal));
                (metrics(&measure, horizontal), hitbox)
            },
            move |_, (next, hitbox), window, _| {
                if next != current {
                    let owner = owner.clone();
                    window.on_next_frame(move |_, cx| {
                        let _ = owner.update(cx, |_, cx| cx.notify());
                    });
                }
                if let Some(hitbox) = hitbox {
                    let scroll = wheel_scroll.clone();
                    let owner = owner.clone();
                    // Handle Shift+wheel before the native scroll element's bubble
                    // handler, so the same event cannot move both axes.
                    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                        if phase != DispatchPhase::Capture
                            || !event.modifiers.shift
                            || !hitbox.should_handle_scroll(window)
                        {
                            return;
                        }
                        let delta = event.delta.pixel_delta(window.line_height());
                        let dx = if delta.x != px(0.) { delta.x } else { delta.y };
                        let mut offset = scroll.offset();
                        offset.x =
                            (offset.x + dx).clamp(-scroll.max_offset().x.max(px(0.)), px(0.));
                        scroll.set_offset(offset);
                        let _ = owner.update(cx, |_, cx| cx.notify());
                        cx.stop_propagation();
                        window.prevent_default();
                    });
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full(),
    );
    if let Some(m) = current {
        let owner_id = cx.entity().entity_id();
        let grab = Rc::new(Cell::new(0.));
        let down_grab = grab.clone();
        let drag_scroll = scroll.clone();
        let thumb = div()
            .id(id)
            .debug_selector(move || id.into())
            .absolute()
            .rounded_full()
            .cursor_pointer()
            .when(horizontal, |v| {
                v.left(px(m.position))
                    .bottom(px(2.))
                    .w(px(m.thumb))
                    .h(px(6.))
            })
            .when(!horizontal, |v| {
                v.top(px(m.position)).right(px(2.)).h(px(m.thumb)).w(px(6.))
            })
            .bg(Hsla { a: 0.4, ..color })
            .hover(move |v| v.bg(Hsla { a: 0.75, ..color }))
            .on_mouse_down(MouseButton::Left, move |e, _, cx| {
                let pointer = f32::from(if horizontal {
                    e.position.x
                } else {
                    e.position.y
                });
                down_grab.set(pointer - m.start - m.position);
                cx.stop_propagation();
            })
            .on_drag(
                ThumbDrag {
                    grab,
                    owner: owner_id,
                    id,
                },
                |drag, _, _, cx| {
                    cx.stop_propagation();
                    cx.new(|_| drag.clone())
                },
            )
            .on_drag_move(cx.listener(move |_, e: &DragMoveEvent<ThumbDrag>, _, cx| {
                let drag = e.drag(cx);
                if drag.owner != owner_id || drag.id != id {
                    return;
                }
                if let Some(m) = metrics(&drag_scroll, horizontal) {
                    let pointer = f32::from(if horizontal {
                        e.event.position.x
                    } else {
                        e.event.position.y
                    });
                    let value = px(m.offset(pointer, e.drag(cx).grab.get()));
                    let mut offset = drag_scroll.offset();
                    if horizontal {
                        offset.x = value;
                    } else {
                        offset.y = value;
                    }
                    drag_scroll.set_offset(offset);
                    cx.notify();
                }
            }));
        overlay = overlay.child(thumb);
    }
    overlay.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proportional_thumb_and_drag_clamp_without_jumping() {
        let m = Metrics::new(10., 200., 800., -400.).unwrap();
        assert_eq!(m.thumb, 40.);
        assert_eq!(m.position, 80.);
        assert_eq!(m.offset(97., 7.), -400.);
        assert_eq!(m.offset(-100., 7.), 0.);
        assert_eq!(m.offset(1000., 7.), -800.);
        assert!(Metrics::new(0., 200., 0., 0.).is_none());
        assert_eq!(Metrics::new(0., 10., 100., 0.).unwrap().offset(5., 0.), 0.);
    }
}
