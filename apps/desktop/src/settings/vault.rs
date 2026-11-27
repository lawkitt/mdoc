//! Settings › Vault: the recovery key (#1091 Decision 22), the Vault key
//! as text and a QR, so a reinstalled system or a new computer can open
//! the documents. The window reads the key only when Show is clicked; the
//! panel holds it in memory while the page shows and wipes it when the
//! page goes. mdoc never writes it to a file, a log or the clipboard.
use super::{Event, Panel, text};
use crate::ui::style::{self, Theme};
use gpui::{AnyElement, Context, Hsla, div, prelude::*, px};
use zeroize::Zeroizing;

/// The recovery key on show.
pub(crate) struct Shown {
    /// Grouped, as the user writes it down.
    text: Zeroizing<String>,
    /// The QR's modules, row by row; 1 is dark.
    modules: Zeroizing<Vec<u8>>,
    width: usize,
}

/// What Settings › Vault shows.
#[derive(Default)]
pub(crate) struct RecoveryUi {
    shown: Option<Shown>,
    /// Why the key can't be shown now.
    error: Option<String>,
}

impl RecoveryUi {
    /// Leaving the page or closing Settings: the key goes.
    pub(super) fn forget(&mut self) {
        self.shown = None;
        self.error = None;
    }
}

/// Side of one QR module, and the white border around the code in
/// modules (the quiet zone scanners need).
const MODULE: f32 = 4.;
const QUIET: usize = 4;

impl Panel {
    /// The window read the key for Show (`Err`: why it can't be shown).
    pub fn show_recovery_key(&mut self, key: Result<[u8; 32], String>, cx: &mut Context<Self>) {
        self.recovery.forget();
        match key.and_then(|key| shown(&key)) {
            Ok(shown) if self.open && self.category == super::Category::Vault => {
                self.recovery.shown = Some(shown);
            }
            Ok(_) => {}
            Err(why) => self.recovery.error = Some(why),
        }
        cx.notify();
    }

    /// Tests: the recovery key on show, as written.
    #[cfg(test)]
    pub(crate) fn recovery_key_shown(&self) -> Option<String> {
        (self.recovery.shown.as_ref()).map(|shown| shown.text.as_str().to_owned())
    }

    /// Tests: the shown QR's width in modules.
    #[cfg(test)]
    pub(crate) fn recovery_qr_width(&self) -> Option<usize> {
        self.recovery.shown.as_ref().map(|shown| shown.width)
    }

    pub(super) fn vault_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let p = theme.pdf_style();
        let muted = p.header_muted;
        let shown = self.recovery.shown.as_ref();
        let button = if shown.is_some() {
            self.scrolled_quiet("recovery-key-hide", text::HIDE, theme, true, cx)
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "recovery-key-hide".into())
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.recovery.forget();
                    cx.notify();
                }))
        } else {
            self.scrolled_control(
                "recovery-key-show",
                text::SHOW_RECOVERY_KEY,
                theme,
                true,
                cx,
            )
            .when(cfg!(test), |v| {
                v.debug_selector(|| "recovery-key-show".into())
            })
            .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::ShowRecoveryKey)))
        };
        let mut section = div().flex().flex_col().child(self.setting_row(
            text::RECOVERY_KEY,
            text::RECOVERY_KEY_DESCRIPTION,
            button,
            theme,
        ));
        if let Some(why) = &self.recovery.error {
            section = section.child(
                div()
                    .py_3()
                    .text_size(px(12.))
                    .text_color(style::markdown_style(theme).alert_caution)
                    .child(why.clone()),
            );
        }
        if let Some(shown) = shown {
            section = section.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_6()
                    .py_4()
                    .border_b_1()
                    .border_color(p.border)
                    .child(key_text(shown, theme))
                    .child(qr(shown)),
            );
        }
        section
            .child(
                div()
                    .py_3()
                    .text_size(px(11.))
                    .text_color(muted)
                    .child(text::RECOVERY_KEY_NOTE),
            )
            .into_any_element()
    }
}

/// The key in its groups, large enough to copy by hand without a mistake.
fn key_text(shown: &Shown, theme: Theme) -> AnyElement {
    let p = theme.pdf_style();
    let mono = style::markdown_style(theme).mono.family.clone();
    let groups: Vec<String> = shown.text.split(' ').map(str::to_owned).collect();
    div()
        .id("recovery-key-text")
        .role(gpui::Role::Label)
        .aria_label(text::RECOVERY_KEY)
        .flex()
        .flex_wrap()
        .gap_x_3()
        .gap_y_2()
        .max_w(px(300.))
        .font_family(mono)
        .text_size(px(18.))
        .text_color(p.header_fg)
        .children(groups.into_iter().map(|group| div().child(group)))
        .into_any_element()
}

/// The QR, dark on white in both themes, so a phone reads it in dark mode
/// too.
fn qr(shown: &Shown) -> AnyElement {
    let modules = shown.modules.clone();
    let width = shown.width;
    let side = px(MODULE * (width + 2 * QUIET) as f32);
    let dark = Hsla::black();
    div()
        .id("recovery-key-qr")
        .role(gpui::Role::Image)
        .aria_label(text::RECOVERY_QR)
        .flex_shrink_0()
        .w(side)
        .h(side)
        .bg(Hsla::white())
        .rounded_sm()
        .child(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let module = px(MODULE);
                    let origin = bounds.origin + gpui::point(module, module) * QUIET as f32;
                    for (row, line) in modules.chunks(width).enumerate() {
                        // One quad per run of dark modules.
                        let mut x = 0;
                        while x < width {
                            if line[x] == 0 {
                                x += 1;
                                continue;
                            }
                            let start = x;
                            while x < width && line[x] == 1 {
                                x += 1;
                            }
                            window.paint_quad(gpui::fill(
                                gpui::Bounds::new(
                                    origin
                                        + gpui::point(module * start as f32, module * row as f32),
                                    gpui::size(module * (x - start) as f32, module),
                                ),
                                dark,
                            ));
                        }
                    }
                },
            )
            .size_full(),
        )
        .into_any_element()
}

fn shown(key: &[u8; 32]) -> Result<Shown, String> {
    let text = Zeroizing::new(mdoc_vault::recovery::format(key));
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M)
        .map_err(text::qr_failed)?;
    let width = code.width();
    let modules = Zeroizing::new(
        code.to_colors()
            .into_iter()
            .map(|c| u8::from(c == qrcode::Color::Dark))
            .collect(),
    );
    Ok(Shown {
        text,
        modules,
        width,
    })
}
