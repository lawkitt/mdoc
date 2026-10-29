//! A focusable div with a layout-independent keyboard outline.
use gpui::{
    A11ySubtreeBuilder, AnyElement, App, AvailableSpace, BorderStyle, Bounds, Corners, Div, Edges,
    Element, ElementId, FocusHandle, GlobalElementId, Hsla, InspectorElementId, Interactivity,
    LayoutId, PaintQuad, Pixels, Stateful, StyleRefinement, Window, accesskit, canvas, div,
    prelude::*, px, transparent_black,
};

/// An ordinary div whose focus indicator is painted above sibling fills.
/// GPUI has no outside-outline style; keeping the handle here lets the ring
/// share the control's identity without adding a wrapper or another Tab stop.
pub struct FocusRing {
    inner: Stateful<Div>,
    accent: Hsla,
    focus: Option<FocusHandle>,
    tab_index: Option<isize>,
    tab_stop: bool,
    deferred: bool,
}

pub fn focus_ring(id: impl Into<ElementId>, accent: Hsla) -> FocusRing {
    FocusRing {
        inner: div().id(id),
        accent,
        focus: None,
        tab_index: None,
        tab_stop: false,
        deferred: true,
    }
}

impl FocusRing {
    /// GPUI's prompt layer prepaints after the deferred pass. Paint its ring
    /// directly in that already topmost layer, with the same geometry.
    /// The host must leave room between controls for their outside strokes.
    pub fn paint_in_place(mut self) -> Self {
        self.deferred = false;
        self
    }

    /// Ring a child that tracks `focus` itself, such as an editor. GPUI
    /// inserts every tracked handle into the Tab order, and the same handle
    /// tracked twice leaves a duplicate stop that `focus_prev` loops on.
    pub fn ring_for(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }
}

struct RingFocus(FocusHandle);

/// Space around a control reserved for the offset and stroke.
pub const FOCUS_RING_OUTSET: f32 = 4.;

fn paint_ring(bounds: Bounds<Pixels>, accent: Hsla, window: &mut Window) {
    window.paint_quad(PaintQuad {
        bounds,
        corner_radii: Corners::all(px(6.)),
        background: transparent_black().into(),
        border_widths: Edges::all(px(2.)),
        border_color: accent,
        border_style: BorderStyle::default(),
    });
}

impl Styled for FocusRing {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}
impl ParentElement for FocusRing {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(elements);
    }
}
impl InteractiveElement for FocusRing {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.inner.interactivity()
    }
    fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self.inner = self.inner.track_focus(focus);
        self
    }
    fn tab_index(mut self, index: isize) -> Self {
        self.tab_index = Some(index);
        self.tab_stop = true;
        self.inner = self.inner.tab_index(index);
        self
    }
    fn tab_stop(mut self, stop: bool) -> Self {
        self.tab_stop = stop;
        self.inner = self.inner.tab_stop(stop);
        self
    }
}
impl StatefulInteractiveElement for FocusRing {}
impl IntoElement for FocusRing {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for FocusRing {
    type RequestLayoutState = <Stateful<Div> as Element>::RequestLayoutState;
    type PrepaintState = <Stateful<Div> as Element>::PrepaintState;
    fn id(&self) -> Option<ElementId> {
        Element::id(&self.inner)
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn a11y_role(&self) -> Option<accesskit::Role> {
        self.inner.a11y_role()
    }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.write_a11y_info(node);
    }
    fn a11y_synthetic_children(
        &mut self,
        state: &mut Self::PrepaintState,
        builder: &mut A11ySubtreeBuilder,
    ) {
        Element::a11y_synthetic_children(&mut self.inner, state, builder);
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        if self.focus.is_none() {
            let focus = window.with_element_state(
                id.expect("focus ring has an id"),
                |state: Option<RingFocus>, _| {
                    let state = state.unwrap_or_else(|| RingFocus(cx.focus_handle()));
                    (state.0.clone(), state)
                },
            );
            let mut focus = focus.tab_stop(self.tab_stop);
            if let Some(index) = self.tab_index {
                focus = focus.tab_index(index);
            }
            self.inner = std::mem::replace(&mut self.inner, div().id("unused")).track_focus(&focus);
            self.focus = Some(focus);
        }
        self.inner.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let hitbox = self
            .inner
            .prepaint(id, inspector, bounds, state, window, cx);
        if self
            .focus
            .as_ref()
            .is_some_and(|focus| focus.is_focused(window))
            && window.last_input_was_keyboard()
            && self.deferred
        {
            let ring_bounds = bounds.dilate(px(FOCUS_RING_OUTSET));
            let accent = self.accent;
            let mask = window.content_mask();
            let mut ring = canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    paint_ring(bounds, accent, window);
                },
            )
            .w(ring_bounds.size.width)
            .h(ring_bounds.size.height)
            .into_any_element();
            ring.layout_as_root(ring_bounds.size.map(AvailableSpace::Definite), window, cx);
            // Two px of clear space, then a two px stroke. Defer only the
            // paint: later selected/hovered siblings cannot cover the ring,
            // and the original scroll mask still clips offscreen controls.
            // App popovers and sheets are themselves deferred. Only the
            // focused control paints a ring, above its owning surface too.
            window.defer_draw(ring, ring_bounds.origin, usize::MAX, Some(mask));
        }
        hitbox
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .paint(id, inspector, bounds, layout, state, window, cx);
        // GPUI prepaints its prompt after the deferred prepaint pass. Its
        // controls paint directly in that topmost layer; the host reserves
        // enough space between prompt buttons for the outside outline.
        if !self.deferred
            && window.last_input_was_keyboard()
            && self
                .focus
                .as_ref()
                .is_some_and(|focus| focus.is_focused(window))
        {
            paint_ring(bounds.dilate(px(FOCUS_RING_OUTSET)), self.accent, window);
        }
    }
}
