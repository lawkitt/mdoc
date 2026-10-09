use gpui::{Hsla, font, hsla, rgb};
use gpui_pdf::PdfStyle;
use mdoc_editor::SyntaxStyle;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

impl Theme {
    pub fn toggle(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    /// The icon names the theme the button will switch to.
    pub fn pdf_style(self) -> PdfStyle {
        match self {
            Self::Dark => PdfStyle {
                bg: rgb(0x1e1e22).into(),
                border: rgb(0x38383f).into(),
                placeholder_bg: rgb(0x35353c).into(),
                placeholder_fg: rgb(0xa0a0aa).into(),
                header_fg: rgb(0xe6e6e6).into(),
                header_muted: rgb(0xa0a0aa).into(),
            },
            Self::Light => PdfStyle {
                bg: rgb(0xfafaf9).into(),
                border: rgb(0xdcdce0).into(),
                placeholder_bg: rgb(0xececef).into(),
                placeholder_fg: rgb(0x666670).into(),
                header_fg: rgb(0x24242b).into(),
                header_muted: rgb(0x666670).into(),
            },
        }
    }

    pub fn search_accent(self) -> Hsla {
        match self {
            Self::Dark => rgb(0x82b9a7),
            Self::Light => rgb(0x336b5c),
        }
        .into()
    }

    pub fn sidebar_bg(self) -> Hsla {
        match self {
            Self::Dark => rgb(0x1b1b1f),
            Self::Light => rgb(0xf3f3f2),
        }
        .into()
    }

    pub fn sidebar_selected(self) -> Hsla {
        match self {
            Self::Dark => rgb(0x293630),
            Self::Light => rgb(0xe2ece7),
        }
        .into()
    }
}

pub fn markdown_style(theme: Theme) -> SyntaxStyle {
    let mut style = SyntaxStyle {
        marker: hsla(0., 0., 0.5, 0.55),       // dimmed gray syntax markers
        code: hsla(0.09, 0.6, 0.72, 1.),       // warm inline code text
        code_bg: hsla(0., 0., 1., 0.06),       // faint code chip background
        link: hsla(0.58, 0.75, 0.66, 1.),      // blue links / wiki-links
        quote: hsla(0., 0., 0.6, 1.),          // muted blockquote text/border
        alert_note: hsla(0.58, 0.9, 0.62, 1.), // GitHub alert blues/greens…
        alert_tip: hsla(0.36, 0.5, 0.48, 1.),
        alert_important: hsla(0.74, 0.85, 0.73, 1.),
        alert_warning: hsla(0.12, 0.7, 0.48, 1.),
        alert_caution: hsla(0.01, 0.9, 0.63, 1.),
        rule: hsla(0., 0., 1., 0.18),                // `---` divider
        mark_bg: hsla(0.13, 1., 0.5, 0.4),           // yellow <mark> highlight
        popover_bg: hsla(0., 0., 0.16, 1.),          // dark menu surface
        popover_border: hsla(0., 0., 0.28, 1.),      // menu border
        popover_fg: hsla(0., 0., 0.9, 1.),           // menu text
        popover_hover: hsla(0.58, 0.75, 0.66, 0.16), // soft accent tint
        popover_divider: hsla(0., 0., 1., 0.18),     // group divider
        popover_danger: gpui::rgb(0xE5484D).into(),  // destructive rows
        mono: font("Menlo"),
    };
    if theme == Theme::Light {
        style.marker = hsla(0., 0., 0.35, 0.7);
        style.code = rgb(0x934115).into();
        style.code_bg = hsla(0., 0., 0., 0.05);
        style.link = rgb(0x175bb5).into();
        style.quote = rgb(0x666670).into();
        style.alert_note = rgb(0x175bb5).into();
        style.alert_tip = rgb(0x287442).into();
        style.alert_important = rgb(0x7942ad).into();
        style.alert_warning = rgb(0x916400).into();
        style.alert_caution = rgb(0xb52c36).into();
        style.rule = hsla(0., 0., 0., 0.18);
        style.mark_bg = hsla(0.13, 1., 0.5, 0.3);
        style.popover_danger = rgb(0xb52c36).into();
    }
    let palette = theme.pdf_style();
    style.popover_bg = palette.bg;
    style.popover_fg = palette.header_fg;
    style.popover_border = palette.border;
    style.popover_hover = palette.placeholder_bg;
    style.popover_divider = palette.border;
    style
}

/// Lightweight tooltip using the same palette as the host chrome.
struct Tip {
    text: String,
    theme: Theme,
}
impl gpui::Render for Tip {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{div, prelude::*, px};
        let palette = self.theme.pdf_style();
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .text_size(px(12.))
            .bg(palette.placeholder_bg)
            .text_color(palette.header_fg)
            .border_1()
            .border_color(palette.border)
            .child(self.text.clone())
    }
}
pub fn tooltip(
    text: String,
    theme: Theme,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView {
    use gpui::AppContext;
    move |_, cx| {
        cx.new(|_| Tip {
            text: text.clone(),
            theme,
        })
        .into()
    }
}
