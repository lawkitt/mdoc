//! "Copied": what a Copy control shows after it copies, worded and drawn
//! once. How long it shows, and what follows it, stay with each control.

use crate::ui::{Control, Icon, style::Theme, text};
use gpui::{Hsla, prelude::*, px};

/// A Copy control's label: "Copied" while `copied`, else its own `label`.
pub fn copy_label(copied: bool, label: &'static str) -> &'static str {
    if copied { text::COPIED } else { label }
}

/// A hand-built Copy control while `copied`: its text in the applied
/// colour, after a 12 px check; else its text in `idle`.
pub fn copied_look(control: Control, copied: bool, idle: Hsla, theme: Theme) -> Control {
    control
        .text_color(if copied { theme.applied_text() } else { idle })
        .when(copied, |button| {
            button.child(
                gpui::svg()
                    .data(Icon::Check.data())
                    .size(px(12.))
                    .text_color(theme.applied_text()),
            )
        })
}
