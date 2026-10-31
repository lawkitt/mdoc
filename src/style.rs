use gpui::{Hsla, font, hsla, rgb};
use gpui_pdf::{PageOverlays, PdfStyle, ThemedPages};
use mdoc_editor::SyntaxStyle;
use mdoc_store::session_store::ThemeChoice;

/// The unified title-bar row (ADR 0048): the content card's top margin and
/// header height, and the traffic lights centred on that header.
pub const CARD_MARGIN: f32 = 8.;
pub const HEADER_HEIGHT: f32 = 44.;
pub const TRAFFIC_LIGHT_X: f32 = 16.;
pub const TRAFFIC_LIGHT_Y: f32 = CARD_MARGIN + HEADER_HEIGHT / 2. - 7.;
/// Room left of the sidebar's top row for the macOS traffic lights.
pub const TRAFFIC_LIGHT_INSET: f32 = if cfg!(target_os = "macos") { 76. } else { 8. };
/// Sidebar toggle (32), two history buttons (28 each), and two 4 px gaps.
pub const WORKSPACE_NAV_WIDTH: f32 = 96.;

/// What a document header shares with the window (ADR 0048): the left room
/// the traffic lights and a collapsed sidebar's toggle need, and the press
/// state that turns a header drag into a window move.
#[derive(Clone, Default)]
pub struct TitleBar {
    pub inset: std::rc::Rc<std::cell::Cell<f32>>,
    pub armed: std::rc::Rc<std::cell::Cell<bool>>,
    /// Sidebar collapsed: headers sit in the window's title row, beside the
    /// traffic lights, and the content gets its own card below (ADR 0048).
    pub detached: std::rc::Rc<std::cell::Cell<bool>>,
    /// Workspace notices waiting (locked cases, a crash report, messages):
    /// headers show a bell with this count (ADR 0048).
    pub notices: std::rc::Rc<std::cell::Cell<usize>>,
    /// The notices popover is open.
    pub notices_open: std::rc::Rc<std::cell::Cell<bool>>,
    /// Where the bell painted last, so a press on it is left to its toggle
    /// instead of closing the popover as an outside click.
    pub notices_bell: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

/// The resolved [`Theme`] for the sidebar's pick (ADR 0048): System follows
/// the OS appearance.
pub fn resolve(choice: ThemeChoice, appearance: gpui::WindowAppearance) -> Theme {
    match choice {
        ThemeChoice::Light => Theme::Light,
        ThemeChoice::Dark => Theme::Dark,
        ThemeChoice::System => match appearance {
            gpui::WindowAppearance::Light | gpui::WindowAppearance::VibrantLight => Theme::Light,
            _ => Theme::Dark,
        },
    }
}

/// The appearance native panels, alerts and menus draw in (macOS, ADR 0076):
/// Light or Dark as chosen; System clears the app's own, so they follow
/// macOS.
#[cfg(any(target_os = "macos", test))]
pub fn app_appearance(choice: ThemeChoice) -> Option<gpui::WindowAppearance> {
    match choice {
        ThemeChoice::System => None,
        ThemeChoice::Light => Some(gpui::WindowAppearance::Light),
        ThemeChoice::Dark => Some(gpui::WindowAppearance::Dark),
    }
}

/// The sidebar dot of a case with this hue (degrees).
pub fn case_color(hue: u32, theme: Theme) -> Hsla {
    hsla(
        hue as f32 / 360.,
        0.65,
        if theme == Theme::Light { 0.32 } else { 0.70 },
        1.,
    )
}

pub fn theme_label(choice: ThemeChoice) -> &'static str {
    match choice {
        ThemeChoice::System => "Theme: System",
        ThemeChoice::Light => "Theme: Light",
        ThemeChoice::Dark => "Theme: Dark",
    }
}

impl Theme {
    /// The icon names the theme the button will switch to.
    pub fn pdf_style(self) -> PdfStyle {
        match self {
            Self::Dark => PdfStyle {
                bg: rgb(0x1e1e22).into(),
                border: rgb(0x38383f).into(),
                placeholder_bg: rgb(0x35353c).into(),
                placeholder_fg: rgb(0xa0a0aa).into(),
                header_fg: rgb(0xe6e6e6).into(),
                focus_accent: self.search_accent(),
                header_muted: rgb(0xa0a0aa).into(),
                overlays: PageOverlays::default(),
                // Dark Original pages (ADR 0035): paper a step above the viewer
                // background so page edges stay visible; ink short of pure white.
                themed_pages: Some(ThemedPages {
                    paper: rgb(0x26262b).into(),
                    ink: rgb(0xd8d8dc).into(),
                    overlays: PageOverlays {
                        search: hsla(0.09, 0.95, 0.5, 0.4),
                        search_current: hsla(0.09, 0.95, 0.5, 0.65),
                        search_border: hsla(0.09, 0.95, 0.62, 0.95),
                        link_hover: hsla(0.58, 0.9, 0.62, 0.25),
                        field_hover: hsla(0.25, 0.8, 0.58, 0.25),
                    },
                }),
            },
            Self::Light => PdfStyle {
                bg: rgb(0xfafaf9).into(),
                border: rgb(0xdcdce0).into(),
                placeholder_bg: rgb(0xececef).into(),
                // 4.5:1 or more on every light ground, the selected
                // sidebar row included (ADR 0067).
                placeholder_fg: rgb(0x5f5f69).into(),
                header_fg: rgb(0x24242b).into(),
                focus_accent: self.search_accent(),
                header_muted: rgb(0x5f5f69).into(),
                overlays: PageOverlays::default(),
                themed_pages: None,
            },
        }
    }

    /// The dimmed window behind a modal dialog (ADR 0064): darker in Dark,
    /// where a light one would barely separate the dialog from the editor.
    pub fn scrim(self) -> Hsla {
        match self {
            Self::Dark => hsla(0., 0., 0., 0.5),
            Self::Light => hsla(0., 0., 0., 0.25),
        }
    }

    /// The floating mention bar's edge: a step stronger than `border`, so it
    /// stays distinct over text in both themes (ADR 0055).
    pub fn popup_border(self) -> Hsla {
        match self {
            Self::Dark => rgb(0x56565f),
            Self::Light => rgb(0xbcbcc4),
        }
        .into()
    }

    pub fn search_accent(self) -> Hsla {
        match self {
            Self::Dark => rgb(0x82b9a7),
            Self::Light => rgb(0x336b5c),
        }
        .into()
    }

    /// A proposed replacement: warm gold chips, rings and markers.
    pub fn proposed(self) -> Hsla {
        match self {
            Self::Dark => rgb(0xdfc07c),
            Self::Light => rgb(0x9a7516),
        }
        .into()
    }

    /// An applied replacement: blue chips, rings, badges and aliases.
    pub fn applied(self) -> Hsla {
        match self {
            Self::Dark => rgb(0x6aa7f5),
            Self::Light => rgb(0x2a62c0),
        }
        .into()
    }

    /// Applied alias text on its own blue tint in the editor: a stronger
    /// shade of [`Self::applied`], 4.5:1 even on the active tint (ADR 0067).
    pub fn applied_text(self) -> Hsla {
        match self {
            Self::Dark => rgb(0x9cc6fa),
            Self::Light => rgb(0x1c4a94),
        }
        .into()
    }

    /// The window ground: the sidebar and the margin around the content card
    /// (ADR 0048). A step darker than the card in both themes.
    pub fn sidebar_bg(self) -> Hsla {
        match self {
            Self::Dark => rgb(0x161619),
            Self::Light => rgb(0xededeb),
        }
        .into()
    }

    /// A ticked checkbox (ADR 0048): a soft, slightly warm grey rather than
    /// ink, still clearly filled in both themes; and the tick drawn on it.
    pub fn checkbox(self) -> (Hsla, Hsla) {
        match self {
            Self::Dark => (hsla(0.08, 0.06, 0.74, 1.), hsla(0., 0., 0.13, 1.)),
            Self::Light => (hsla(0.08, 0.05, 0.36, 1.), hsla(0., 0., 1., 1.)),
        }
    }

    /// The content card's hairline edge on the ground.
    pub fn card_border(self) -> Hsla {
        match self {
            Self::Dark => hsla(0., 0., 1., 0.06),
            Self::Light => hsla(0., 0., 0., 0.07),
        }
    }

    /// A category's colour (ADR 0048): the dot on its panel header and the
    /// underline of its mentions. Muted, readable on both cards; never state.
    pub fn category(self, category: mdoc_pii::Category) -> Hsla {
        use mdoc_pii::Category::*;
        let (hue, sat) = match category {
            Person => (0.10, 0.62),
            Organization => (0.48, 0.45),
            Email => (0.58, 0.50),
            Phone => (0.80, 0.35),
            Address => (0.62, 0.45),
            Date => (0.74, 0.35),
            Identity => (0.01, 0.50),
            Tax => (0.92, 0.40),
            Bank => (0.33, 0.40),
            Other => (0.0, 0.0),
        };
        match self {
            // Mention words and aliases are drawn in it: 4.5:1 (ADR 0067).
            Self::Dark => hsla(hue, sat, 0.70, 1.),
            Self::Light => hsla(hue, sat, 0.32, 1.),
        }
    }

    /// The selected sidebar row: neutral, a step above the ground (ADR 0048).
    pub fn sidebar_selected(self) -> Hsla {
        match self {
            Self::Dark => rgb(0x2a2a30),
            Self::Light => rgb(0xdfdfdc),
        }
        .into()
    }
}

impl Theme {
    /// Text on the notices bell's amber badge: dark on the dark theme's
    /// lighter amber, white on the light theme's deeper one (ADR 0067).
    pub fn badge_fg(self) -> Hsla {
        match self {
            Self::Dark => rgb(0x1e1e22).into(),
            Self::Light => gpui::white(),
        }
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
        popover_danger: gpui::rgb(0xF47C7F).into(),  // destructive rows
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
        tip_body(self.text.clone(), self.theme)
    }
}

/// A tooltip read on every frame, for a control whose click changes what
/// it says while the pointer stays on it.
struct LiveTip(std::rc::Rc<dyn Fn() -> (String, Theme)>);
impl gpui::Render for LiveTip {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let (text, theme) = (self.0)();
        tip_body(text, theme)
    }
}

pub fn live_tooltip(
    read: impl Fn() -> (String, Theme) + 'static,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView {
    use gpui::AppContext;
    let read: std::rc::Rc<dyn Fn() -> (String, Theme)> = std::rc::Rc::new(read);
    move |_, cx| cx.new(|_| LiveTip(read.clone())).into()
}

fn tip_body(text: String, theme: Theme) -> impl gpui::IntoElement {
    {
        use gpui::{div, prelude::*, px};
        let palette = theme.pdf_style();
        div()
            .when(cfg!(test), |v| {
                let selector = format!("tooltip-{text}");
                v.debug_selector(move || selector.clone())
            })
            .px_2()
            .py_1()
            .rounded_md()
            // Long explanations wrap instead of running off the window.
            .max_w(px(320.))
            .text_size(px(12.))
            .bg(palette.placeholder_bg)
            .text_color(palette.header_fg)
            .border_1()
            .border_color(palette.border)
            .child(text)
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

/// WCAG 2.2 contrast over the theme in light and dark (ADR 0067): text at
/// 4.5:1 on the grounds it is drawn on (1.4.3), the marks a control needs
/// to be seen at 3:1 (1.4.11). Alpha colors are blended onto their ground.
#[cfg(test)]
mod contrast_tests {
    use super::*;
    use mdoc_pii::Category;

    /// The UI's text sizes in px, smallest first. Every `text_size(px(…))`
    /// in the app is one of these steps.
    const TYPE_RAMP: [f32; 9] = [10., 11., 12., 13., 14., 15., 16., 18., 26.];

    /// A colour's name, the colour and the grounds it is drawn on.
    type Pair<'a> = (&'a str, Hsla, &'a [(&'a str, Hsla)]);

    fn luminance(color: Hsla, ground: Hsla) -> f32 {
        let (c, g) = (gpui::Rgba::from(color), gpui::Rgba::from(ground));
        let channel = |c: f32, g: f32| {
            let v = c * color.a + g * (1. - color.a);
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(c.r, g.r) + 0.7152 * channel(c.g, g.g) + 0.0722 * channel(c.b, g.b)
    }

    /// `color` painted over an opaque `ground`, as one opaque colour.
    fn over(color: Hsla, ground: Hsla) -> Hsla {
        let (c, g) = (gpui::Rgba::from(color), gpui::Rgba::from(ground));
        let mix = |c: f32, g: f32| c * color.a + g * (1. - color.a);
        gpui::Rgba {
            r: mix(c.r, g.r),
            g: mix(c.g, g.g),
            b: mix(c.b, g.b),
            a: 1.,
        }
        .into()
    }

    fn ratio(fg: Hsla, bg: Hsla) -> f32 {
        let (a, b) = (luminance(fg, bg), luminance(bg, bg));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Every failing (theme, name, ground, ratio) below `minimum`.
    fn failures(theme: Theme, minimum: f32, pairs: &[Pair]) -> Vec<String> {
        let mut out = Vec::new();
        for (name, color, grounds) in pairs {
            for (ground, bg) in grounds.iter() {
                let r = ratio(*color, *bg);
                if r < minimum {
                    out.push(format!("{theme:?}: {name} on {ground} is {r:.2}:1"));
                }
            }
        }
        out
    }

    #[test]
    fn all_case_swatches_meet_nontext_contrast_in_both_themes() {
        let mut failed = Vec::new();
        for theme in [Theme::Light, Theme::Dark] {
            let p = theme.pdf_style();
            let grounds = [
                ("card", p.bg),
                ("sidebar", theme.sidebar_bg()),
                ("hover", p.placeholder_bg),
                ("selected", theme.sidebar_selected()),
            ];
            for swatch in mdoc_store::cases::CaseColor::ALL {
                failed.extend(failures(
                    theme,
                    3.,
                    &[(swatch.name(), case_color(swatch.hue(), theme), &grounds)],
                ));
            }
        }
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    #[test]
    fn theme_text_and_controls_meet_wcag_contrast() {
        let mut failed = Vec::new();
        for theme in [Theme::Dark, Theme::Light] {
            let p = theme.pdf_style();
            let m = markdown_style(theme);
            let card = ("card", p.bg);
            let hover = ("hover", p.placeholder_bg);
            let chrome: &[(&str, Hsla)] = &[
                card,
                ("sidebar", theme.sidebar_bg()),
                hover,
                ("selected row", theme.sidebar_selected()),
            ];
            let rows: &[(&str, Hsla)] = &[card, hover];
            let page: &[(&str, Hsla)] = &[card];
            // Text, 1.4.3. Syntax markers (`**`, `#`) are left out: they are
            // dimmed on purpose and repeat what the rendered style shows.
            failed.extend(failures(
                theme,
                4.5,
                &[
                    ("header text", p.header_fg, chrome),
                    ("muted text", p.header_muted, chrome),
                    ("placeholder text", p.placeholder_fg, rows),
                    ("focused control text", theme.search_accent(), rows),
                    (
                        "selected theme segment text",
                        p.header_fg,
                        &[("selected segment", theme.sidebar_selected())],
                    ),
                    ("applied alias", theme.applied(), rows),
                    ("destructive menu row", m.popover_danger, rows),
                    ("tooltip text", p.header_fg, &[hover]),
                    ("link", m.link, page),
                    ("inline code", m.code, page),
                    ("quote", m.quote, page),
                    ("note", m.alert_note, page),
                    ("tip", m.alert_tip, page),
                    ("important", m.alert_important, page),
                    ("warning", m.alert_warning, page),
                    ("caution", m.alert_caution, page),
                    (
                        "bell count",
                        theme.badge_fg(),
                        &[("amber badge", m.alert_warning)],
                    ),
                ],
            ));
            // Non-text, 1.4.11: what a control needs to be seen. Borders and
            // hairlines (the card, dividers, the mention bar's edge) are
            // decorative: content and shadow identify those surfaces.
            let (fill, tick) = theme.checkbox();
            let categories = [
                Category::Person,
                Category::Organization,
                Category::Email,
                Category::Phone,
                Category::Address,
                Category::Date,
                Category::Identity,
                Category::Tax,
                Category::Bank,
            ]
            .map(|c| (format!("{c:?} category"), theme.category(c)));
            let its_box = [("its box", fill)];
            let theme_segment = [("selected segment", theme.sidebar_selected())];
            let mut marks: Vec<Pair> = vec![
                ("ticked checkbox", fill, rows),
                ("checkbox tick", tick, &its_box),
                ("proposed ring", theme.proposed(), rows),
                ("applied ring", theme.applied(), rows),
                ("focus accent", theme.search_accent(), rows),
                (
                    "selected theme segment border",
                    theme.search_accent(),
                    &theme_segment,
                ),
            ];
            for (name, color) in &categories {
                marks.push((name.as_str(), *color, page));
            }
            // Applied aliases in the editor sit on their own blue tint, at
            // rest (16%) and active (34%), as `pii::ui` paints them.
            let tint = |a: f32| {
                over(
                    Hsla {
                        a,
                        ..theme.applied()
                    },
                    p.bg,
                )
            };
            failed.extend(failures(
                theme,
                4.5,
                &[(
                    "applied alias in the editor",
                    theme.applied_text(),
                    &[("its tint", tint(0.16)), ("its active tint", tint(0.34))],
                )],
            ));
            // Mention words and popup aliases are drawn in category colours.
            let category_text: Vec<Pair> = categories
                .iter()
                .map(|(name, color)| (name.as_str(), *color, rows))
                .collect();
            failed.extend(failures(theme, 4.5, &category_text));
            failed.extend(failures(theme, 3., &marks));
        }
        assert!(
            failed.is_empty(),
            "contrast below WCAG 2.2 AA:\n{}",
            failed.join("\n")
        );
    }

    #[test]
    fn focus_ring_meets_non_text_contrast_on_every_control_ground() {
        let mut failed = Vec::new();
        for theme in [Theme::Dark, Theme::Light] {
            let p = theme.pdf_style();
            let accent = theme.search_accent();
            // The outside stroke can cross a neighbouring selected/hovered
            // row as well as its own control's fill (ADR 0074).
            let grounds = [
                ("card and menus", p.bg),
                ("sidebar and state cards", theme.sidebar_bg()),
                ("hover and toggle-on fill", p.placeholder_bg),
                ("selected sidebar row", theme.sidebar_selected()),
                (
                    "selected Settings model",
                    over(Hsla { a: 0.05, ..accent }, p.bg),
                ),
                (
                    "selected recognition language",
                    over(Hsla { a: 0.08, ..accent }, p.bg),
                ),
            ];
            failed.extend(failures(theme, 3., &[("focus ring", accent, &grounds)]));
        }
        assert!(
            failed.is_empty(),
            "focus contrast below 3:1:\n{}",
            failed.join("\n")
        );
    }

    #[test]
    fn the_type_ramp_rises_and_every_text_size_is_on_it() {
        assert!(TYPE_RAMP.windows(2).all(|w| w[0] < w[1]), "ramp rises");
        const { assert!(TYPE_RAMP[0] >= 10., "nothing below 10 px") };
        // Every literal size in the app's and crates' sources is a step.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut stack = vec![root.join("src"), root.join("crates")];
        let mut off = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy();
                if path.is_dir() {
                    if name != "examples" && name != "target" {
                        stack.push(path);
                    }
                    continue;
                }
                if !name.ends_with(".rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (n, line) in text.lines().enumerate() {
                    // A drawing of another app's dialog keeps that app's sizes.
                    if line.contains("type-ramp: illustration") {
                        continue;
                    }
                    for part in line.split("text_size(px(").skip(1) {
                        let Some(number) = part.split(')').next() else {
                            continue;
                        };
                        let Ok(size) = number.trim_end_matches('.').parse::<f32>() else {
                            continue;
                        };
                        if !TYPE_RAMP.contains(&size) {
                            off.push(format!("{}:{} uses {size} px", path.display(), n + 1));
                        }
                    }
                }
            }
        }
        assert!(
            off.is_empty(),
            "sizes off the type ramp:\n{}",
            off.join("\n")
        );
    }
}

#[cfg(test)]
mod appearance_tests {
    use super::*;
    use gpui::WindowAppearance;

    #[test]
    fn native_panels_follow_the_choice_and_system_follows_the_os() {
        assert_eq!(app_appearance(ThemeChoice::System), None);
        assert_eq!(
            app_appearance(ThemeChoice::Light),
            Some(WindowAppearance::Light)
        );
        assert_eq!(
            app_appearance(ThemeChoice::Dark),
            Some(WindowAppearance::Dark)
        );
    }
}
