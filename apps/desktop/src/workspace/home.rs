//! Home: its view with the drop zone, the quick actions and the Continue
//! strip, whose rows, like the palette's search over the Vault, are read
//! from the manifest's summaries in memory, so no document record is opened.

use super::Tab;
use super::Workspace;
use super::home;
use super::palette::key;
use crate::document::DocumentView;
use crate::ui;
use crate::ui::style;
use crate::ui::style::Theme;
use crate::workspace::New;
use crate::workspace::Open;
use crate::workspace::PasteAiResponse;
use crate::workspace::PseudonymizeClipboard;
use gpui::Animation;
use gpui::AnimationExt;
use gpui::AnyElement;
use gpui::App;
use gpui::Context;
use gpui::Entity;
use gpui::Hsla;
use gpui::Window;
use gpui::actions;
use gpui::div;
use gpui::prelude::*;
use gpui::px;
use mdoc_import::import;
use mdoc_store::document_id::VaultId;
use mdoc_vault::ReplyOwner;
use mdoc_vault::{DocumentSummary, ProjectSummary, ReplySummary};
use std::time::Duration;
use std::{cmp::Reverse, collections::HashMap};

/// How many cards the Continue strip shows.
pub(super) const STRIP_CARDS: usize = 3;
/// How many documents a palette search lists.
pub(super) const SEARCH_LIMIT: usize = 50;

/// One card of the Continue strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Card {
    /// A document that needs review; `proposed` is a live view's count
    /// (none for a closed document, whose count the manifest doesn't keep).
    Document {
        summary: DocumentSummary,
        proposed: Option<usize>,
    },
    Reply(ReplySummary),
}

impl Card {
    /// Its document's or reply's id.
    pub(super) fn id(&self) -> &str {
        match self {
            Self::Document { summary, .. } => &summary.id,
            Self::Reply(reply) => &reply.id,
        }
    }

    /// When it last changed, in seconds since the epoch.
    pub(super) fn at(&self) -> u64 {
        match self {
            Self::Document { summary, .. } => summary.updated,
            Self::Reply(reply) => reply.created,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Strip {
    /// Shown at all: the Vault holds a document or a reply.
    pub(super) shown: bool,
    pub(super) cards: Vec<Card>,
    /// Every document that needs review, shown or not.
    pub(super) need_review: usize,
}

/// Whether `summary` needs review: it was never saved Ready (desktop marks
/// a document scanned only by a Ready save), or it has proposals left
/// ([`proposals`]). A document kept from AI never does. The Project page's
/// Documents segment uses the same rule.
pub(super) fn needs_review(summary: &DocumentSummary, proposed: Option<usize>) -> bool {
    !summary.exclude_from_ai && (proposed.is_some_and(|n| n > 0) || !summary.scanned)
}

/// Proposals left in `summary`'s document: what its live view has now,
/// `live`, or else what its last save counted.
pub(super) fn proposals(summary: &DocumentSummary, live: Option<usize>) -> Option<usize> {
    live.or_else(|| (summary.counts).map(|counts| counts.proposed as usize))
        .filter(|&n| n > 0)
}

/// The Continue strip: documents that need review and saved replies,
/// newest first, at most [`STRIP_CARDS`]. `live` holds each live view's
/// proposals left, by document id.
pub(super) fn strip(
    documents: &[DocumentSummary],
    replies: &[ReplySummary],
    live: &HashMap<String, usize>,
) -> Strip {
    let mut cards: Vec<Card> = documents
        .iter()
        .filter_map(|summary| {
            let proposed = proposals(summary, live.get(&summary.id).copied());
            needs_review(summary, proposed).then(|| Card::Document {
                summary: summary.clone(),
                proposed,
            })
        })
        .collect();
    let need_review = cards.len();
    cards.extend(replies.iter().cloned().map(Card::Reply));
    // Stable: equal times keep documents before replies, each in the
    // Vault's order.
    cards.sort_by_key(|card| Reverse(card.at()));
    cards.truncate(STRIP_CARDS);
    Strip {
        shown: !documents.is_empty() || !replies.is_empty(),
        cards,
        need_review,
    }
}

/// Seconds since the epoch now, as the Vault stamps its records.
pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// How long ago `at` was, at `now` (both seconds since the epoch): "just
/// now", "5 min ago", "3 h ago", "yesterday", "4 days ago", then the date.
pub(super) fn age(at: u64, now: u64) -> String {
    let seconds = now.saturating_sub(at);
    match seconds {
        0..60 => "just now".into(),
        60..3_600 => format!("{} min ago", seconds / 60),
        3_600..86_400 => format!("{} h ago", seconds / 3_600),
        86_400..172_800 => "yesterday".into(),
        172_800..604_800 => format!("{} days ago", seconds / 86_400),
        _ => chrono::DateTime::from_timestamp(at as i64, 0).map_or_else(String::new, |date| {
            let date = date.with_timezone(&chrono::Local);
            let year = |d: &chrono::DateTime<chrono::Local>| chrono::Datelike::year(d);
            let this_year = chrono::DateTime::from_timestamp(now as i64, 0)
                .is_some_and(|today| year(&today.with_timezone(&chrono::Local)) == year(&date));
            date.format(if this_year { "%-d %b" } else { "%-d %b %Y" })
                .to_string()
        }),
    }
}

/// What a palette query finds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Found {
    pub(super) documents: Vec<DocumentSummary>,
    pub(super) projects: Vec<ProjectSummary>,
}

/// The query's words, lowercased; none for an empty query.
fn words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// How well `words` match `text` (lowercased), best first: `text` starts
/// with the query, every word starts a word of `text`, `text` contains every
/// word; `None` when a word is missing.
fn rank(words: &[String], text: &str) -> Option<u8> {
    if !words.iter().all(|word| text.contains(word.as_str())) {
        return None;
    }
    if text.starts_with(&words.join(" ")) {
        return Some(0);
    }
    let starts = |word: &String| {
        text.split(|c: char| !c.is_alphanumeric())
            .any(|part| part.starts_with(word.as_str()))
    };
    Some(if words.iter().all(starts) { 1 } else { 2 })
}

/// Search the Vault's titles and Project names for `query`. A document
/// matches when each word is in its title or its Project's name; those
/// matched by title come first, best match then most recently updated.
/// Projects match by name, best match first, then in `projects`' order.
/// An empty query finds nothing.
pub(super) fn search(
    query: &str,
    projects: &[ProjectSummary],
    documents: &[DocumentSummary],
) -> Found {
    let words = words(query);
    if words.is_empty() {
        return Found::default();
    }
    let names: HashMap<&str, String> = projects
        .iter()
        .map(|project| (project.id.as_str(), project.name.to_lowercase()))
        .collect();
    let mut found: Vec<(u8, &DocumentSummary)> = documents
        .iter()
        .filter_map(|summary| {
            let title = summary.title.to_lowercase();
            if let Some(rank) = rank(&words, &title) {
                return Some((rank, summary));
            }
            let project = (summary.project.as_deref())
                .and_then(|id| names.get(id))
                .map_or("", String::as_str);
            let each = words
                .iter()
                .all(|word| title.contains(word.as_str()) || project.contains(word.as_str()));
            each.then_some((3, summary))
        })
        .collect();
    found.sort_by_key(|(rank, summary)| (*rank, Reverse(summary.updated)));
    let mut ranked: Vec<(u8, &ProjectSummary)> = projects
        .iter()
        .filter_map(|project| Some((rank(&words, &project.name.to_lowercase())?, project)))
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    Found {
        documents: (found.into_iter().take(SEARCH_LIMIT))
            .map(|(_, summary)| summary.clone())
            .collect(),
        projects: ranked.into_iter().map(|(_, p)| p.clone()).collect(),
    }
}

actions!(mdoc_shell, [ShowHome, PasteAsNewTab]);

impl Workspace {
    // ---- Home state -------------------------------------------------------

    /// The hidden blank tab under Home: never edited, never listed.
    pub(super) fn is_placeholder(&self, tab: &Tab, cx: &App) -> bool {
        tab.record().blank_disposable
            && tab.view().is_none_or(|view| {
                let view = view.read(cx);
                view.lifecycle.blank_disposable()
                    && view.preview.source().is_none()
                    && view.editor.read(cx).text().is_empty()
            })
    }
    pub(super) fn showing_home(&self, cx: &App) -> bool {
        self.project_ui.page.is_none()
            && self.reply_page.is_none()
            && (self.tabs.home()
                || self
                    .tabs
                    .active_tab()
                    .is_some_and(|tab| self.is_placeholder(tab, cx)))
    }
    pub(super) fn showing_overview(&self, cx: &App) -> bool {
        self.project_ui.page.is_some() || self.reply_page.is_some() || self.showing_home(cx)
    }
    pub(super) fn show_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_home_as(super::sidebar::navigation::Arrival::Visit, window, cx);
    }
    /// Show Home, recording it in Back/Forward as `arrival` says.
    pub(super) fn show_home_as(
        &mut self,
        arrival: super::sidebar::navigation::Arrival,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(old) = self.active_view() {
            old.update(cx, |view, _| view.pii.dismiss_popup());
        }
        self.reply_page = None;
        self.project_ui.page = None;
        self.end_rename(true, cx);
        self.tabs.show_home();
        self.note_destination(arrival, cx);
        self.drop_if_empty_new(self.tabs.active(), window, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }
    // ---- Home -----------------------------------------------------------------

    pub(super) fn home_view(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let detached = self.title.detached.get();
        let bar = ui::title_drag(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(style::HEADER_HEIGHT))
                .pl(px(16. + self.title.inset.get()))
                .pr_2()
                .gap_2()
                .when(detached, |v| {
                    v.h(px(style::CARD_MARGIN + style::HEADER_HEIGHT))
                        .pt(px(style::CARD_MARGIN))
                })
                .when(!detached, |v| v.border_b_1().border_color(p.border))
                .text_size(px(13.)),
            self.title.armed.clone(),
        )
        .child(div().font_weight(gpui::FontWeight::MEDIUM).child("Home"))
        .child(div().flex_1())
        .children(ui::notice_bell(&self.title, theme).map(|bell| ui::no_drag(div().child(bell))));
        let header = ui::title_bar(bar, (!detached).then_some(p.border), window, theme);
        let banner = self.whats_new(cx);
        let body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .children(banner)
            .child(
                div()
                    .id("home-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.shell.home_scroll)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .px_6()
                            .pt(px(56.))
                            .pb(px(48.))
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(680.))
                                    .flex()
                                    .flex_col()
                                    .gap_4()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_size(px(26.))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .child("Hello"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(14.))
                                                    .text_color(p.header_muted)
                                                    .child(
                                                        "What would you like to prepare for AI?",
                                                    ),
                                            ),
                                    )
                                    .child(self.drop_zone(cx))
                                    .child(self.home_actions(cx))
                                    .children(self.home_strip(cx)),
                            ),
                    ),
            );
        // Collapsed sidebar: the header sits in the title row and Home in its
        // own card below. The header comes after the body in
        // element order, shown on top by the reversed column, so Home's
        // first Tab stop is Open files…, not the header's bell (U25).
        div()
            .id("home")
            .track_focus(self.focus_regions.home())
            .when(cfg!(test), |v| v.debug_selector(|| "home".into()))
            .size_full()
            .flex()
            .flex_col_reverse()
            .map(|v| {
                if detached {
                    v.child(ui::card_surface(body.ml(px(style::CARD_MARGIN)), theme))
                } else {
                    v.child(body)
                }
            })
            .child(header)
            .into_any_element()
    }
    /// One-time "What's new" after an update.
    fn whats_new(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::app::updater::BannerKind;
        let banner = crate::app::updater::state(cx).banner()?;
        if banner.kind != BannerKind::Updated {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let version = banner.version.clone();
        Some(
            div()
                .flex()
                .items_center()
                .gap_3()
                .px_4()
                .py_2()
                .bg(p.placeholder_bg)
                .text_size(px(13.))
                .child(
                    div()
                        .flex_1()
                        .child(format!("mdoc {version} is installed.")),
                )
                .child(
                    ui::link_button("whats-new", "What's new", theme, true).on_click(
                        move |_, _, cx| cx.open_url(&crate::app::updater::release_url(&version)),
                    ),
                )
                .child(
                    ui::control("dismiss-whats-new", "×", theme, true)
                        .aria_label("Dismiss")
                        .on_click(|_, _, cx| crate::app::updater::dismiss_banner(cx)),
                )
                .into_any_element(),
        )
    }
    fn drop_zone(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let accent = theme.search_accent();
        let active = self.shell.drop.is_some_and(|n| n > 0);
        let refused = self.shell.drop == Some(0);
        let ink = if active { accent } else { p.header_muted };
        let layer = |data: &'static [u8], color: gpui::Hsla| {
            gpui::svg()
                .data(data)
                .absolute()
                .inset_0()
                .size_full()
                .text_color(color)
        };
        let illustration = div()
            .relative()
            .w(px(160.))
            .h(px(112.))
            .child(layer(
                include_bytes!("../../resources/ui/drop-page.svg"),
                ink,
            ))
            .child(layer(
                include_bytes!("../../resources/ui/drop-bars.svg"),
                if active {
                    accent
                } else {
                    Hsla {
                        a: 0.35,
                        ..p.header_muted
                    }
                },
            ))
            .child(layer(
                include_bytes!("../../resources/ui/drop-disc.svg"),
                if active { accent } else { p.header_fg },
            ))
            .child(layer(
                include_bytes!("../../resources/ui/drop-lock.svg"),
                p.bg,
            ));
        let illustration = if active {
            illustration
                .with_animation(
                    ("drop-lift", self.shell.drop_generation),
                    Animation::new(Duration::from_millis(180)).with_easing(gpui::ease_out_quint()),
                    |v, delta| v.mt(px(-6. * delta)).mb(px(6. * delta)),
                )
                .into_any_element()
        } else {
            illustration.into_any_element()
        };
        // Sealed with another key: the zone asks for the recovery key.
        if self.vault.wrong_key_root().is_some() {
            return self.recovery_panel(cx);
        }
        // No Vault this run: nothing can be added, and the zone says why.
        let unavailable = self.vault.unavailable();
        let title = match self.shell.drop {
            _ if unavailable.is_some() => "Documents are unavailable".to_string(),
            Some(0) => "These files can't be opened".to_string(),
            Some(1) => "Drop to open".to_string(),
            Some(n) => format!("Drop {n} files"),
            None => "Drop documents here".to_string(),
        };
        div()
            .id("home-drop")
            .when(cfg!(test), |v| v.debug_selector(|| "home-drop".into()))
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .pt_6()
            .pb_5()
            .rounded_xl()
            .border_1()
            .border_dashed()
            .border_color(Hsla {
                a: 0.45,
                ..p.header_muted
            })
            .when(active, |v| {
                v.border_2()
                    .border_color(accent)
                    .bg(Hsla { a: 0.05, ..accent })
            })
            .when(refused, |v| {
                v.border_color(style::markdown_style(theme).alert_caution)
            })
            .child(illustration)
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if active { accent } else { p.header_fg })
                    .child(title),
            )
            .child(
                ui::filled_button("home-open", "Open files…", theme, unavailable.is_none())
                    .when(cfg!(test), |v| v.debug_selector(|| "home-open".into()))
                    .mt_1()
                    .child(ui::key_hint(key(&Open)))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(Open), cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .text_center()
                    .px_4()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .map(|v| match unavailable {
                        Some(why) => v.max_w(px(460.)).child(why.to_owned()),
                        None => v
                            .child(mdoc_import::import::supported_formats())
                            .child("Several at once each become a document in Unfiled"),
                    }),
            )
            .into_any_element()
    }
    fn home_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let machine = if cfg!(target_os = "macos") {
            "this Mac"
        } else {
            "this computer"
        };
        let detail = format!(
            "Detection, conversion and Projects run on {machine}. mdoc goes online only \
             to check for updates and to download models you choose."
        );
        let intake = self.vault.unavailable().is_none();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                ui::primary_button("home-new", "New document", theme, intake)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-new".into()))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(New), cx)),
            )
            .child(
                ui::primary_button("home-paste", "Paste text", theme, intake)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-paste".into()))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(PasteAsNewTab), cx)),
            )
            .child(
                ui::primary_button("home-restore", "Paste AI reply", theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-restore".into()))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(PasteAiResponse), cx)
                    }),
            )
            .child(
                ui::primary_button("home-clipboard", "Pseudonymize clipboard", theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-clipboard".into()))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(PseudonymizeClipboard), cx)
                    }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("empty-page-local")
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "empty-page-local".into())
                    })
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .aria_label(format!("Documents stay on {machine}. {detail}"))
                    // Above the note, right-aligned with it: a pointer tooltip
                    // at the window's edge would flip over the buttons beside
                    // it (U16).
                    .relative()
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if this.shell.local_note_hover != *hovered {
                            this.shell.local_note_hover = *hovered;
                            cx.notify();
                        }
                    }))
                    .child(
                        gpui::svg()
                            .data(include_bytes!("../../resources/ui/lock.svg"))
                            .size(px(13.))
                            .text_color(p.header_muted),
                    )
                    .child(format!("Documents stay on {machine}"))
                    .when(self.shell.local_note_hover, |v| {
                        v.child(
                            gpui::deferred(
                                div()
                                    .absolute()
                                    .right_0()
                                    .bottom(gpui::relative(1.))
                                    .w(px(300.))
                                    // Clear of the taller buttons in the row.
                                    .pb(px(10.))
                                    .flex()
                                    .justify_end()
                                    .child(style::tip_body(detail, theme)),
                            )
                            .with_priority(1),
                        )
                    }),
            )
            .into_any_element()
    }
    /// Each live view's proposals left, by its Vault document's id.
    fn live_proposals(&self, cx: &App) -> HashMap<String, usize> {
        (self.tabs.iter())
            .filter_map(|tab| {
                let view = tab.view()?.read(cx);
                let document = self.live_document(tab, cx)?;
                Some((document.as_str().to_string(), view.pii.review().remaining()))
            })
            .collect()
    }
    /// The Continue strip as it stands, with the summaries it was read
    /// from; `None` while the Vault is closed.
    pub(super) fn home_strip_now(&self, cx: &App) -> Option<(home::Strip, Vec<DocumentSummary>)> {
        let vault = self.vault.vault()?;
        let documents = vault.documents();
        let strip = home::strip(&documents, &vault.replies(), &self.live_proposals(cx));
        Some((strip, documents))
    }
    /// Continue: documents that need review and saved replies, newest first.
    fn home_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (strip, documents) = self.home_strip_now(cx)?;
        if !strip.shown {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let projects: HashMap<String, ProjectSummary> = (self.projects.summaries().into_iter())
            .map(|project| (project.id.clone(), project))
            .collect();
        let documents_shown = (strip.cards.iter())
            .filter(|card| matches!(card, home::Card::Document { .. }))
            .count();
        let more = match strip.need_review {
            0 if !strip.cards.is_empty() => Some("Nothing needs review".to_string()),
            n if n <= documents_shown => None,
            1 => Some("1 needs review".to_string()),
            n => Some(format!("{n} need review")),
        };
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .pt_2()
            .child(home_label("Continue", theme).pt_0())
            .child(div().flex_1())
            .children(more.map(|more| {
                div()
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "home-strip-more".into())
                    })
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(more)
            }));
        let strip_box = div()
            .id("home-strip")
            .when(cfg!(test), |v| v.debug_selector(|| "home-strip".into()))
            .flex()
            .flex_col()
            .gap_2()
            .child(header);
        if strip.cards.is_empty() {
            return Some(
                strip_box
                    .child(
                        div()
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "home-strip-empty".into())
                            })
                            .text_size(px(13.))
                            .text_color(p.header_muted)
                            .child("Nothing needs review."),
                    )
                    .into_any_element(),
            );
        }
        // A document's place: its Project's dot and name, or Unfiled.
        let place = |project: Option<&str>| -> (Option<Hsla>, String) {
            match project.and_then(|id| projects.get(id)) {
                Some(project) => {
                    let link = super::ProjectLink {
                        id: project.id.clone(),
                        name: project.name.clone(),
                        colour: project.colour,
                    };
                    (
                        Some(style::project_color(link.hue(), theme)),
                        project.name.clone(),
                    )
                }
                None => (None, "Unfiled".to_string()),
            }
        };
        let now = home::now();
        let shown = strip.cards.len();
        let mut row = div().flex().gap_3();
        for (i, card) in strip.cards.into_iter().enumerate() {
            let age = home::age(card.at(), now);
            let (badge, title, (dot, place), status) = match &card {
                home::Card::Document { summary, proposed } => (
                    super::sidebar::tree_ui::kind_label(summary),
                    summary.title.clone(),
                    place(summary.project.as_deref()),
                    match proposed {
                        Some(1) => "1 to review".to_string(),
                        Some(n) => format!("{n} to review"),
                        None => "Not reviewed".to_string(),
                    },
                ),
                home::Card::Reply(reply) => {
                    let project = match &reply.owner {
                        ReplyOwner::Project(id) => Some(id.as_str()),
                        ReplyOwner::Document(id) => (documents.iter())
                            .find(|summary| &summary.id == id)
                            .and_then(|summary| summary.project.as_deref()),
                    };
                    (
                        "REPLY",
                        reply.title.clone(),
                        place(project),
                        "Saved reply".to_string(),
                    )
                }
            };
            let label = format!("{title}, {place}, {status}, {age}");
            let focus = (self.shell.card_focus.borrow_mut())
                .entry(card.id().to_string())
                .or_insert_with(|| cx.focus_handle().tab_stop(true))
                .clone();
            let view =
                crate::ui::focus_ring(("home-card", i), theme)
                    .when(cfg!(test), |v| {
                        v.debug_selector(move || format!("home-card-{i}"))
                    })
                    .role(gpui::Role::Button)
                    .aria_label(label)
                    .key_context("UiControl")
                    .tab_index(0)
                    .track_focus(&focus)
                    .flex_1()
                    .flex_basis(px(0.))
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border)
                    .text_size(px(13.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.placeholder_bg))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(10.))
                                    .text_color(p.header_muted)
                                    .child(badge),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(12.))
                            .text_color(p.header_muted)
                            .children(dot.map(|dot| {
                                div().size(px(8.)).rounded_full().bg(dot).flex_shrink_0()
                            }))
                            .child(div().min_w_0().truncate().child(place)),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.header_muted)
                            .truncate()
                            .child(format!("{status} · {age}")),
                    );
            let view = match card {
                home::Card::Document { summary, .. } => {
                    let id = VaultId::parse(&summary.id);
                    view.on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(id) = id.clone() {
                            this.open_vault_document(id, window, cx);
                        }
                    }))
                }
                home::Card::Reply(reply) => {
                    let id = reply.id;
                    view.on_click(
                        cx.listener(move |this, _, window, cx| this.open_reply(&id, window, cx)),
                    )
                }
            };
            row = row.child(view);
        }
        // Fewer cards keep a card's width.
        for _ in shown..home::STRIP_CARDS {
            row = row.child(div().flex_1().flex_basis(px(0.)));
        }
        Some(strip_box.child(row).into_any_element())
    }
    /// A file drag anywhere over the window while Home shows.
    pub(super) fn home_drag(
        &mut self,
        event: &gpui::DragMoveEvent<gpui::ExternalPaths>,
        cx: &mut Context<Self>,
    ) {
        if !self.showing_overview(cx) {
            return;
        }
        let over = event.bounds.contains(&event.event.position);
        let count = over.then(|| {
            event
                .drag(cx)
                .paths()
                .iter()
                .filter(|path| import::droppable(path))
                .count()
        });
        if self.shell.drop != count {
            if self.shell.drop.is_none() {
                self.shell.drop_generation += 1;
            }
            self.shell.drop = count;
            cx.notify();
        }
    }
    pub(super) fn paste_as_new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.intake_refused(window, cx) {
            return;
        }
        let item = cx.read_from_clipboard();
        // A copied file carries its path as text; the editor's Paste skips it.
        let has_files = item.as_ref().is_some_and(|item| {
            item.entries()
                .iter()
                .any(|entry| matches!(entry, gpui::ClipboardEntry::ExternalPaths(_)))
        });
        let Some(text) = item
            .and_then(|item| item.text())
            .filter(|text| !has_files && !text.trim().is_empty())
        else {
            self.notices.post("The clipboard has no text to paste.");
            cx.notify();
            return;
        };
        // Straight into the new buffer: a dispatched Paste would arrive before
        // the new editor holds focus and reach nothing (issue #300).
        let id = self.new_tab(window, cx);
        if let Some(view) = self.tabs.get(id).and_then(|tab| tab.view()).cloned() {
            view.update(cx, |view, cx| {
                // An ordinary document, not a new one.
                view.session.pasted = true;
                view.editor
                    .update(cx, |editor, cx| editor.insert_pasted(text, window, cx))
            });
        }
    }
    /// Home's clipboard actions belong to a document; with none open, say so.
    pub(super) fn clipboard_document(
        &mut self,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<DocumentView>> {
        let real = self
            .tabs
            .active_tab()
            .is_some_and(|tab| !self.is_placeholder(tab, cx));
        if !real {
            // The notice opens even over an earlier one.
            let effect = self.notices.post_now(format!(
                "{action} works from a pseudonymized document. Open one, or open a Project, first."
            ));
            self.carry_out_notices(effect);
            cx.notify();
            return None;
        }
        // Leave Home so document-level feedback and any error are visible.
        if self.showing_overview(cx) {
            self.activate(self.tabs.active(), window, cx);
        }
        self.active_view()
    }
}

fn home_label(label: &'static str, theme: Theme) -> gpui::Div {
    div()
        .pt_2()
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.pdf_style().header_muted)
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_vault::{DocumentOrigin, ReplyOwner, SourceKind};

    fn document(id: &str, title: &str, project: Option<&str>, updated: u64) -> DocumentSummary {
        DocumentSummary {
            id: id.into(),
            project: project.map(Into::into),
            title: title.into(),
            source_kind: SourceKind::Pdf,
            origin: DocumentOrigin::Ordinary,
            exclude_from_ai: false,
            scanned: false,
            created: updated,
            updated,
            source_sha256: None,
            counts: None,
        }
    }

    fn project(id: &str, name: &str) -> ProjectSummary {
        ProjectSummary {
            id: id.into(),
            name: name.into(),
            colour: None,
            pinned: false,
            created: 0,
            updated: 0,
        }
    }

    fn reply(id: &str, created: u64) -> ReplySummary {
        ReplySummary {
            id: id.into(),
            owner: ReplyOwner::Document("d".into()),
            title: format!("Reply {id}"),
            source_app: None,
            created,
        }
    }

    fn ids(strip: &Strip) -> Vec<String> {
        (strip.cards.iter())
            .map(|card| match card {
                Card::Document { summary, .. } => summary.id.clone(),
                Card::Reply(reply) => reply.id.clone(),
            })
            .collect()
    }

    #[test]
    fn proposals_come_from_the_live_view_or_else_the_last_save() {
        let mut lease = document("a", "Lease", None, 1);
        assert_eq!(proposals(&lease, None), None);
        lease.counts = Some(mdoc_vault::ReviewCounts {
            proposed: 2,
            ..Default::default()
        });
        assert_eq!(proposals(&lease, None), Some(2));
        assert_eq!(
            proposals(&lease, Some(0)),
            None,
            "the live view says none now"
        );
        assert_eq!(proposals(&lease, Some(3)), Some(3));
    }

    #[test]
    fn a_document_needs_review_until_saved_ready_and_while_its_view_has_proposals() {
        let mut lease = document("a", "Lease", None, 1);
        assert!(needs_review(&lease, None));
        assert!(needs_review(&lease, Some(0)));
        lease.scanned = true;
        assert!(!needs_review(&lease, None));
        assert!(!needs_review(&lease, Some(0)));
        assert!(needs_review(&lease, Some(2)));
        lease.exclude_from_ai = true;
        assert!(!needs_review(&lease, Some(2)));
    }

    #[test]
    fn the_strip_merges_documents_and_replies_newest_first_and_counts_the_rest() {
        let mut done = document("done", "Done", None, 50);
        done.scanned = true;
        let mut kept = document("kept", "Kept from AI", None, 60);
        kept.exclude_from_ai = true;
        let documents = [
            document("old", "Old", None, 10),
            document("new", "New", Some("p"), 40),
            document("live", "Live", None, 20),
            done,
            kept,
        ];
        let live = HashMap::from([("live".to_string(), 3), ("done".to_string(), 0)]);
        let strip = strip(&documents, &[reply("r", 30)], &live);
        assert!(strip.shown);
        assert_eq!(ids(&strip), ["new", "r", "live"]);
        assert_eq!(strip.need_review, 3);
        assert_eq!(
            strip.cards[2],
            Card::Document {
                summary: documents[2].clone(),
                proposed: Some(3)
            }
        );
    }

    #[test]
    fn the_strip_hides_with_an_empty_vault_and_is_empty_with_nothing_to_review() {
        assert_eq!(strip(&[], &[], &HashMap::new()), Strip::default());
        let mut done = document("done", "Done", None, 5);
        done.scanned = true;
        let strip = strip(&[done], &[], &HashMap::new());
        assert!(strip.shown && strip.cards.is_empty() && strip.need_review == 0);
    }

    #[test]
    fn search_matches_each_word_in_the_title_or_the_project_name() {
        let projects = [project("p", "Smith v Jones"), project("q", "Lease review")];
        let documents = [
            document("a", "Lease.pdf", Some("p"), 1),
            document("b", "Schedule.docx", Some("p"), 2),
            document("c", "Notes", None, 3),
        ];
        let found = search("smith lease", &projects, &documents);
        assert_eq!(found.documents, [documents[0].clone()]);
        assert!(found.projects.is_empty());
        let found = search("JONES", &projects, &documents);
        assert_eq!(
            found.documents,
            [documents[1].clone(), documents[0].clone()]
        );
        assert_eq!(found.projects, [projects[0].clone()]);
        assert_eq!(search("  ", &projects, &documents), Found::default());
        assert_eq!(search("nothing", &projects, &documents), Found::default());
    }

    #[test]
    fn search_ranks_prefix_then_word_starts_then_contains_then_project_name() {
        let projects = [project("p", "Lease matters"), project("q", "Old leases")];
        let documents = [
            document("project", "Notes", Some("p"), 9),
            document("contains", "Release", None, 8),
            document("word", "Signed lease", None, 1),
            document("word-newer", "Draft lease", None, 2),
            document("prefix", "Lease agreement", None, 0),
        ];
        let found = search("lease", &projects, &documents);
        let order: Vec<_> = found.documents.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(
            order,
            ["prefix", "word-newer", "word", "contains", "project"]
        );
        let order: Vec<_> = found.projects.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(order, ["p", "q"]);
    }

    #[test]
    fn an_age_reads_in_minutes_hours_days_then_a_date() {
        let now = 1_800_000_000;
        assert_eq!(age(now - 5, now), "just now");
        assert_eq!(age(now - 300, now), "5 min ago");
        assert_eq!(age(now - 3 * 3_600, now), "3 h ago");
        assert_eq!(age(now - 30 * 3_600, now), "yesterday");
        assert_eq!(age(now - 4 * 86_400, now), "4 days ago");
        assert!(age(now - 400 * 86_400, now).ends_with("2025"));
        // A clock set back never shows a future time.
        assert_eq!(age(now + 50, now), "just now");
    }

    #[test]
    fn search_lists_at_most_the_limit() {
        let documents: Vec<_> = (0..SEARCH_LIMIT as u64 + 5)
            .map(|i| document(&i.to_string(), "Lease", None, i))
            .collect();
        let found = search("lease", &[], &documents);
        assert_eq!(found.documents.len(), SEARCH_LIMIT);
        assert_eq!(found.documents[0].updated, SEARCH_LIMIT as u64 + 4);
    }
}
