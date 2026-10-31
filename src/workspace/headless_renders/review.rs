//! Pseudonymization review: the Replacements panel, the word popup, model
//! setup, Before/After, the merge hint (ADR 0080) and a running scan.
use super::{Driver, Result};
use crate::{
    document_view::DocumentView,
    pii::ui::{PiiApplyAll, PiiClosePopup},
};
use gpui::{Entity, Focusable};
use mdoc_store::settings;
use std::path::Path;

/// More menu, review, its focus rings, the word popup and model setup.
pub(super) fn scenes(
    d: &mut Driver,
    view: &Entity<DocumentView>,
    path: &Path,
    detections: &[mdoc_pii::Detection],
) -> Result<()> {
    // Nothing is pseudonymized yet: Copy for AI says why it is off.
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.more_open = true;
            window.focus(&v.more_menu_focus, cx);
            cx.notify();
        })
    })?;
    d.capture("more-menu")?;
    d.change(|_, window, cx| view.update(cx, |v, cx| v.close_more(window, cx)))?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            // A Tab in the editor edits text. Restore the fixture before review.
            let text = std::fs::read_to_string(path).expect("copied fixture");
            v.editor.update(cx, |e, cx| e.set_text(text.clone(), cx));
            v.pii
                .review_mut()
                .ingest(&text, detections.to_vec())
                .expect("fixture review");
            v.review_changed(cx);
            v.toggle_replacements_panel(window, cx);
        })
    })?;
    d.capture("review")?;
    d.change(|_, window, cx| {
        let region = view.read(cx).toolbar_region.clone();
        window.focus(&region, cx);
    })?;
    d.key("tab")?;
    let toggle = d.change(|_, window, cx| {
        assert!(view.read(cx).replacements_open());
        window.focused(cx).expect("first toolbar control")
    })?;
    d.focus_pair("toggle-on", toggle)?;
    let sidebar = d.change(|w, _, _| {
        w.sidebar_row_focus
            .borrow()
            .get(&w.tabs.active())
            .expect("selected sidebar row")
            .clone()
    })?;
    d.focus_pair("sidebar-row", sidebar)?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.inspector_share = true;
            window.focus(&v.replacements_focus(), cx);
            cx.notify();
        });
    })?;
    d.settle()?;
    d.key("tab")?;
    let review_tab = d.change(|_, window, cx| window.focused(cx).expect("Review tab"))?;
    d.focus_pair("beside-selected-tab", review_tab)?;
    d.change(|_, _, cx| {
        view.update(cx, |v, cx| {
            v.inspector_share = false;
            cx.notify();
        })
    })?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            let variant = &v.pii.review().variants()[0];
            let id = v
                .pii
                .review()
                .annotation_id(variant.id, &variant.mentions[0])
                .unwrap();
            v.activate_annotation(id, window, cx);
        })
    })?;
    d.capture("mention-popup")?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| v.close_pii_popup(&PiiClosePopup, window, cx))
    })?;
    selection_pill(d, view)?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.pii.show_setup(true);
            window.focus(&v.replacements_focus(), cx);
            cx.notify();
        })
    })?;
    d.capture("setup-consent")?;
    d.change(|_, _, cx| {
        view.update(cx, |v, cx| {
            v.preferences
                .borrow_mut()
                .current
                .as_mut()
                .expect("sandbox preferences")
                .pseudonymization
                .model = settings::PiiModel::Fp32;
            cx.notify();
        })
    })?;
    d.capture("setup-consent-fp32")?;
    d.change(|_, _, cx| {
        view.update(cx, |v, cx| {
            v.preferences
                .borrow_mut()
                .current
                .as_mut()
                .expect("sandbox preferences")
                .pseudonymization
                .model = settings::PiiModel::Fp16;
            v.pii.show_setup(false);
            cx.notify();
        })
    })?;
    Ok(())
}

/// "Replace · Person ▾" beside a selection, then its category menu open
/// (ADR 0024). The pill's Tab walk indents in the editor; undo restores it.
fn selection_pill(d: &mut Driver, view: &Entity<DocumentView>) -> Result<()> {
    let select = |d: &mut Driver| {
        d.change(|_, window, cx| {
            view.update(cx, |v, cx| {
                let text = v.editor.read(cx).text().to_owned();
                let start = text.find("Services Agreement").expect("fixture title");
                window.focus(&v.editor.read(cx).focus_handle(cx), cx);
                v.editor.update(cx, |e, cx| {
                    e.set_selection(start..start + "Services Agreement".len(), cx)
                });
                v.sync_selection_action(cx);
                let history = v.editor.read(cx).history_id();
                let choice = v.editor.read(cx).selection_action().cloned();
                let choice = choice.and_then(|a| a.choice).expect("split pill");
                assert_eq!(choice.label.as_ref(), "Person");
                history
            })
        })
    };
    let before = select(d)?;
    d.capture("selection-pill")?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            for _ in 0..8 {
                if v.editor.read(cx).history_id() == before {
                    break;
                }
                v.editor.update(cx, |e, cx| e.undo_step(window, cx));
            }
            assert_eq!(v.editor.read(cx).history_id(), before, "Tab edits undone");
        })
    })?;
    select(d)?;
    d.change(|_, window, cx| view.update(cx, |v, cx| v.toggle_selection_menu(window, cx)))?;
    d.capture("selection-category-menu")?;
    d.key("escape")?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            let editor = v.editor.read(cx).focus_handle(cx);
            assert!(editor.is_focused(window), "Esc hands focus back");
            v.editor.update(cx, |e, cx| e.set_cursor(0, cx));
        })
    })?;
    Ok(())
}

/// Apply all, then Before/After.
pub(super) fn before_after(d: &mut Driver, view: &Entity<DocumentView>) -> Result<()> {
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.pii.set_decision_clock(|| "14:02".into());
            v.apply_all_pii(&PiiApplyAll, window, cx);
            v.toggle_compare(window, cx);
        })
    })?;
    d.capture("before-after")?;
    d.change(|_, window, cx| view.update(cx, |v, cx| v.close_compare(window, cx)))?;
    Ok(())
}

/// ADR 0080: a synthetic proposal pair, rendered on both shipped targets
/// in each theme. No scan, model, case key or native window is involved.
pub(super) fn merge_hint(d: &mut Driver) -> Result<()> {
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            v.editor.update(cx, |e, cx| {
                e.set_text("Maya Bennett signed. Ms Bennett agreed.", cx)
            });
        });
    })?;
    d.settle()?;
    d.change(|w, window, cx| {
        w.active_view().unwrap().update(cx, |v, cx| {
            let text = v.editor.read(cx).text().to_string();
            v.pii
                .review_mut()
                .ingest(
                    &text,
                    vec![mdoc_pii::Detection {
                        range: 0..12,
                        category: mdoc_pii::Category::Person,
                        score: 1.,
                        recognizer: mdoc_pii::Recognizer::Model,
                    }],
                )
                .expect("merge hint fixture");
            assert_eq!(v.pii.review().merge_hints().len(), 1);
            v.review_changed(cx);
            if !v.replacements_open() {
                v.toggle_replacements_panel(window, cx);
            }
            window.focus(&v.replacements_focus(), cx);
        });
    })?;
    d.capture("merge-hint")
}

/// A scan of a long document under way (#309): skeleton rows, "Scanning…"
/// in the footer, Rescan off and the toolbar's window count. No detector
/// runs; the scan is stopped after the capture.
pub(super) fn scanning(d: &mut Driver) -> Result<()> {
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            v.editor.update(cx, |e, cx| {
                e.set_text(
                    "# Services Agreement

Maya Bennett and Northbridge Legal Ltd agree.",
                    cx,
                )
            });
        });
    })?;
    d.settle()?;
    d.change(|w, window, cx| {
        w.active_view().unwrap().update(cx, |v, cx| {
            v.pii.show_scan(Some((2, 12)));
            window.focus(&v.replacements_focus(), cx);
            cx.notify();
        });
    })?;
    d.capture("review-scanning")?;
    d.change(|w, window, cx| {
        w.active_view().unwrap().update(cx, |v, cx| {
            v.pii.show_scan(None);
            v.toggle_replacements_panel(window, cx);
        });
    })
}
