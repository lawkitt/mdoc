//! Settings › Detection's user-wide Always and Never lists (ADR 0092),
//! edited by hand: add a typed wording, remove one. They are kept with the
//! saved cases under the OS-held key, never in `settings.json`, so the
//! window loads and saves them; while cases are locked they wait too.
use super::{Event, Panel};
use crate::{markdown_search::SearchInput, style::Theme, ui};
use gpui::{AnyElement, Context, Entity, Focusable, actions, div, prelude::*, px};
use mdoc_store::lists::Lists;

actions!(model_settings, [AddUserListEntry]);

/// The lists and their two inputs.
pub struct UserListsUi {
    /// `None` until the window has read them, or while cases are locked.
    lists: Option<Lists>,
    always: Entity<SearchInput>,
    never: Entity<SearchInput>,
}

impl UserListsUi {
    pub(super) fn new(theme: std::rc::Rc<std::cell::Cell<Theme>>, cx: &mut Context<Panel>) -> Self {
        let input = |label: &'static str, cx: &mut Context<Panel>| {
            cx.new(|cx| {
                SearchInput::new(cx)
                    .with_theme(theme.clone())
                    .with_key_context("SettingsInput")
                    .with_placeholder(label)
                    .with_label(label)
            })
        };
        Self {
            lists: None,
            always: input("Add a wording to always replace", cx),
            never: input("Add a wording to never replace", cx),
        }
    }
}

impl Panel {
    /// The user-wide lists scans use; empty while not read.
    pub fn user_lists(&self) -> &Lists {
        static EMPTY: Lists = Lists {
            always: Vec::new(),
            never: Vec::new(),
        };
        self.user_lists.lists.as_ref().unwrap_or(&EMPTY)
    }
    /// The window read the lists (`None`: cases are locked).
    pub fn set_user_lists(&mut self, lists: Option<Lists>, cx: &mut Context<Self>) {
        self.user_lists.lists = lists;
        cx.notify();
    }
    fn change_user_lists(
        &mut self,
        change: impl FnOnce(&mut Lists) -> bool,
        cx: &mut Context<Self>,
    ) {
        let Some(lists) = self.user_lists.lists.as_mut() else {
            return;
        };
        if change(lists) {
            cx.emit(Event::UserListsChanged(lists.clone()));
            cx.notify();
        }
    }
    /// Add the typed wording to Always (in the category a manual mark of it
    /// would guess) or Never.
    pub(crate) fn add_user_list_entry(&mut self, always: bool, cx: &mut Context<Self>) {
        let input = if always {
            self.user_lists.always.clone()
        } else {
            self.user_lists.never.clone()
        };
        let wording = input.read(cx).value().trim().to_string();
        if wording.is_empty() {
            return;
        }
        self.change_user_lists(
            |lists| {
                if always {
                    let category = mdoc_detect::guess_category(&wording, 0..wording.len());
                    lists.add_always(&wording, category)
                } else {
                    lists.add_never(&wording)
                }
            },
            cx,
        );
        input.update(cx, |input, cx| input.reset(cx));
    }
    pub(crate) fn remove_user_list_entry(
        &mut self,
        always: bool,
        wording: &str,
        cx: &mut Context<Self>,
    ) {
        self.change_user_lists(
            |lists| {
                if always {
                    lists.remove_always(wording)
                } else {
                    lists.remove_never(wording)
                }
            },
            cx,
        );
    }
    /// Type `wording` into a list's input, for tests and renders.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn type_user_list_entry(
        &mut self,
        always: bool,
        wording: &str,
        cx: &mut Context<Self>,
    ) {
        let input = if always {
            &self.user_lists.always
        } else {
            &self.user_lists.never
        };
        input.update(cx, |input, cx| input.set_value(wording.into(), cx));
    }

    pub(super) fn user_lists_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let p = theme.pdf_style();
        let section = div()
            .id("settings-user-lists")
            .flex()
            .flex_col()
            .gap_2()
            .py_3()
            .border_b_1()
            .border_color(p.border)
            .child(div().text_size(px(13.)).child("Always and never replace"))
            .child(div().text_size(px(11.)).text_color(p.header_muted).child(
                "Scans in every document propose these wordings, or never do. A case's own \
                 lists, from what you keep and mark in it, come first.",
            ));
        let Some(lists) = &self.user_lists.lists else {
            return section
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(12.))
                                .text_color(p.header_muted)
                                .child(
                                    "Your lists are kept with your saved cases, which are locked.",
                                ),
                        )
                        .child(
                            self.scrolled_quiet("user-lists-unlock", "Unlock…", theme, true, cx)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "user-lists-unlock".into())
                                })
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::UnlockLists))),
                        ),
                )
                .into_any_element();
        };
        let rows = |always: bool,
                    entries: Vec<(String, Option<&'static str>)>,
                    cx: &mut Context<Self>| {
            let list = if always { "always" } else { "never" };
            let input = if always {
                self.user_lists.always.clone()
            } else {
                self.user_lists.never.clone()
            };
            let mut column = div().flex().flex_col().gap_1().child(
                div()
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(if always {
                        "Always replace"
                    } else {
                        "Never replace"
                    }),
            );
            if entries.is_empty() {
                column = column.child(div().text_size(px(12.)).text_color(p.header_muted).child(if always {
                    "No wordings yet."
                } else {
                    "No wordings yet. Courts and public bodies are left as they are unless you add them to Always."
                }));
            }
            for (index, (wording, category)) in entries.into_iter().enumerate() {
                let id = format!("user-{list}-remove-{index}");
                let remove = wording.clone();
                column = column.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(13.))
                                .truncate()
                                .child(wording.clone()),
                        )
                        .children(category.map(|label| {
                            div()
                                .text_size(px(11.))
                                .text_color(p.header_muted)
                                .child(label)
                        }))
                        .child(
                            self.scrolled_quiet(
                                gpui::SharedString::from(id.clone()),
                                "Remove",
                                theme,
                                true,
                                cx,
                            )
                            .aria_label(format!("Remove “{wording}”"))
                            .when(cfg!(test), move |v| v.debug_selector(move || id.clone()))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.remove_user_list_entry(always, &remove, cx)
                                },
                            )),
                        ),
                );
            }
            let add_id = format!("user-{list}-add");
            column
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .key_context("UserListInput")
                        .on_action(cx.listener(move |this, _: &AddUserListEntry, _, cx| {
                            this.add_user_list_entry(always, cx);
                            cx.stop_propagation();
                        }))
                        .child(ui::reveal_focus(
                            div()
                                .flex_1()
                                .border_1()
                                .rounded_md()
                                .border_color(p.border)
                                .bg(p.bg)
                                .px_2()
                                .py_1()
                                .child(input.clone()),
                            input.read(cx).focus_handle(cx),
                            self.scroll.clone(),
                        ))
                        .child(
                            self.scrolled_quiet(
                                gpui::SharedString::from(add_id.clone()),
                                "Add",
                                theme,
                                true,
                                cx,
                            )
                            .aria_label(if always {
                                "Add to Always replace"
                            } else {
                                "Add to Never replace"
                            })
                            .when(cfg!(test), move |v| {
                                v.debug_selector(move || add_id.clone())
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.add_user_list_entry(always, cx),
                            )),
                        ),
                )
                .into_any_element()
        };
        let always = lists
            .always
            .iter()
            .map(|e| (e.wording.clone(), Some(e.category().label())))
            .collect();
        let never = lists.never.iter().map(|w| (w.clone(), None)).collect();
        let always = rows(true, always, cx);
        let never = rows(false, never, cx);
        section
            .child(div().pt_1().child(always))
            .child(div().pt_2().child(never))
            .into_any_element()
    }
}
