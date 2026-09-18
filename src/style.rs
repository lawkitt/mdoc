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
    pub fn toggle_label(self) -> &'static str {
        match self {
            Self::Dark => "☀ Light",
            Self::Light => "☾ Dark",
        }
    }

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

    pub fn error_bg(self) -> Hsla {
        match self {
            Self::Dark => rgb(0x542e32),
            Self::Light => rgb(0xffe4e6),
        }
        .into()
    }
}

pub fn markdown_style(theme: Theme) -> SyntaxStyle {
    let mut style = SyntaxStyle {
        block_label: None,
        block_label_gen: 0,
        block_ref_count: None,
        marker: hsla(0., 0., 0.5, 0.55), // dimmed gray syntax markers
        code: hsla(0.09, 0.6, 0.72, 1.), // warm inline code text
        code_bg: hsla(0., 0., 1., 0.06), // faint code chip background
        link: hsla(0.58, 0.75, 0.66, 1.), // blue links / wiki-links
        tag: hsla(0.33, 0.45, 0.62, 1.), // green tags
        quote: hsla(0., 0., 0.6, 1.),    // muted blockquote text/border
        alert_note: hsla(0.58, 0.9, 0.62, 1.), // GitHub alert blues/greens…
        alert_tip: hsla(0.36, 0.5, 0.48, 1.),
        alert_important: hsla(0.74, 0.85, 0.73, 1.),
        alert_warning: hsla(0.12, 0.7, 0.48, 1.),
        alert_caution: hsla(0.01, 0.9, 0.63, 1.),
        alert_icons: None,
        rule: hsla(0., 0., 1., 0.18),                // `---` divider
        mark_bg: hsla(0.13, 1., 0.5, 0.4),           // yellow <mark> highlight
        popover_bg: hsla(0., 0., 0.16, 1.),          // dark menu surface
        popover_border: hsla(0., 0., 0.28, 1.),      // menu border
        popover_fg: hsla(0., 0., 0.9, 1.),           // menu text
        popover_hover: hsla(0.58, 0.75, 0.66, 0.16), // soft accent tint
        popover_divider: hsla(0., 0., 1., 0.18),     // group divider
        popover_danger: gpui::rgb(0xE5484D).into(),  // destructive rows
        mono: font("Menlo"),
        property_icon: None,
    };
    if theme == Theme::Light {
        style.marker = hsla(0., 0., 0.35, 0.7);
        style.code = rgb(0x934115).into();
        style.code_bg = hsla(0., 0., 0., 0.05);
        style.link = rgb(0x175bb5).into();
        style.tag = rgb(0x287442).into();
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
