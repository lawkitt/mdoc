//! The one-table invariant (ADR 0073): every bound user-facing action is in
//! the table, and the table is what the app binds.
use super::*;
use gpui::{Keystroke, TestAppContext};

#[test]
fn workspace_navigation_is_everywhere_with_platform_keys_and_text_context_guards() {
    for (label, mac, other, action) in [
        (
            "Back",
            "cmd-[",
            "alt-left",
            Box::new(workspace::Back) as Box<dyn Action>,
        ),
        (
            "Forward",
            "cmd-]",
            "alt-right",
            Box::new(workspace::Forward) as Box<dyn Action>,
        ),
    ] {
        let row = SHORTCUTS.iter().find(|row| row.label == label).unwrap();
        assert_eq!(row.group, Group::Everywhere);
        assert_eq!(row.keys(true), [mac]);
        assert_eq!(row.keys(false), [other]);
        for (platform, key, context) in [(true, mac, "Workspace"), (false, other, CHROME)] {
            let bindings = specs(platform);
            let navigation: Vec<_> = bindings
                .iter()
                .filter(|spec| spec.action.partial_eq(&*action))
                .collect();
            assert_eq!(navigation.len(), 1);
            assert_eq!(navigation[0].keys, key);
            assert_eq!(navigation[0].context, Some(context));
        }
    }
}

/// Back/Forward keys as each platform's whole keymap resolves them, checked
/// on any OS (ADR 0061, 0081): on Windows and Linux Alt+← / Alt+→ stay word
/// moves in the editor, do nothing in a text field and navigate elsewhere;
/// on macOS ⌘[ / ⌘] navigate from the editor too and ⌥←/→ never navigate.
#[test]
fn navigation_keys_resolve_by_focus_on_both_platforms_keymaps() {
    let context = |names: &[&str]| -> Vec<gpui::KeyContext> {
        names
            .iter()
            .map(|name| gpui::KeyContext::parse(name).unwrap())
            .collect()
    };
    let editor = context(&["Workspace", "Editor"]);
    let field = context(&["Workspace", "TextInput SearchInput"]);
    let chrome = context(&["Workspace", "UiControl"]);
    for mac in [true, false] {
        // Registered as bind_all_keys does: the editor's plumbing, then the table.
        let mut bindings = mdoc_editor::key_bindings(mac);
        bindings.extend(specs(mac).into_iter().map(Spec::binding));
        let keymap = gpui::Keymap::new(bindings);
        let first = |key: &str, stack: &[gpui::KeyContext]| {
            let (found, _) = keymap.bindings_for_input(&[Keystroke::parse(key).unwrap()], stack);
            found.first().map(|binding| binding.action().name())
        };
        let (back, forward) = (workspace::Back.name(), workspace::Forward.name());
        if mac {
            for stack in [&editor, &field, &chrome] {
                assert_eq!(first("cmd-[", stack), Some(back));
                assert_eq!(first("cmd-]", stack), Some(forward));
            }
            assert_eq!(first("alt-left", &chrome), None);
            assert_eq!(first("alt-right", &chrome), None);
        } else {
            assert_eq!(
                first("alt-left", &editor),
                Some(mdoc_editor::WordLeft.name())
            );
            assert_eq!(
                first("alt-right", &editor),
                Some(mdoc_editor::WordRight.name())
            );
            assert_ne!(first("alt-left", &field), Some(back));
            assert_ne!(first("alt-right", &field), Some(forward));
            assert_eq!(first("alt-left", &chrome), Some(back));
            assert_eq!(first("alt-right", &chrome), Some(forward));
            assert_eq!(first("cmd-[", &chrome), None);
        }
    }
}

/// Plumbing, by action: Tab inside a control group, moving inside menus,
/// choosers and the palette's list, caret and deletion basics in the editor
/// and text fields, Esc dismissing the editor's own transient state, and the
/// OS character palette. Anything else bound must come from the table.
const PLUMBING: &[&str] = &[
    "mdoc_editor::Backspace",
    "mdoc_editor::Delete",
    "mdoc_editor::Left",
    "mdoc_editor::Right",
    "mdoc_editor::Up",
    "mdoc_editor::Down",
    "mdoc_editor::Home",
    "mdoc_editor::End",
    "mdoc_editor::SelectLeft",
    "mdoc_editor::SelectRight",
    "mdoc_editor::SelectUp",
    "mdoc_editor::SelectDown",
    "mdoc_editor::ShowCharacterPalette",
    "mdoc_editor::Dismiss",
    "mdoc_search_input::Backspace",
    "mdoc_search_input::Delete",
    "mdoc_search_input::Left",
    "mdoc_search_input::Right",
    "mdoc_search_input::SelectLeft",
    "mdoc_search_input::SelectRight",
    "mdoc_search_input::Home",
    "mdoc_search_input::End",
    "mdoc_search_input::ShowCharacterPalette",
    "ui::NextControl",
    "ui::PreviousControl",
    "model_settings::NextSettingsField",
    "model_settings::NextCategory",
    "model_settings::PreviousCategory",
    "model_settings::CommitNumber",
    "model_settings::PreviousSettingsField",
    "mdoc::PiiNextChoice",
    "mdoc::PiiPreviousChoice",
    "mdoc::PiiOpenChoice",
    "mdoc_shell::PaletteUp",
    "mdoc_shell::PaletteDown",
    "mdoc_shell::PaletteConfirm",
    "pdf_view::MenuNext",
    "pdf_view::MenuPrevious",
];

/// A binding as GPUI parsed it: keystroke, action and context.
#[derive(Debug, PartialEq)]
struct Key(Keystroke, &'static str, Option<String>);

fn bound(binding: &KeyBinding) -> Key {
    assert_eq!(binding.keystrokes().len(), 1, "no chords");
    Key(
        binding.keystrokes()[0].inner().clone(),
        binding.action().name(),
        binding.predicate().map(|p| p.to_string()),
    )
}

fn listed(spec: &Spec) -> Key {
    Key(
        Keystroke::parse(&spec.keys).expect("valid keystroke"),
        spec.action.name(),
        spec.context
            .map(|c| KeyBindingContextPredicate::parse(c).unwrap().to_string()),
    )
}

#[gpui::test]
fn every_bound_user_facing_action_is_in_the_table(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::bind_all_keys(cx);
        crate::menus::install(cx);
        let table: Vec<Key> = specs(MAC).iter().map(listed).collect();
        let keymap = cx.key_bindings();
        let keymap: Vec<Key> = keymap.borrow().bindings().map(bound).collect();
        let missing: Vec<&Key> = keymap
            .iter()
            .filter(|key| !table.contains(key) && !PLUMBING.contains(&key.1))
            .collect();
        assert!(
            missing.is_empty(),
            "bound, but neither in the shortcuts table nor plumbing: {missing:#?}"
        );
        let unbound: Vec<&Key> = table.iter().filter(|key| !keymap.contains(key)).collect();
        assert!(
            unbound.is_empty(),
            "in the shortcuts table, but no bind_keys registers it: {unbound:#?}"
        );
    });
}

/// The crates' own keymaps (for their tests and other hosts) bind exactly
/// what the table lists for them, plus plumbing, on both platforms.
#[test]
fn crate_keymaps_agree_with_the_table_on_both_platforms() {
    for mac in [true, false] {
        for (owner, keymap) in [
            (Owner::Editor, mdoc_editor::key_bindings(mac)),
            (Owner::Pdf, gpui_pdf::key_bindings(mac)),
        ] {
            let table: Vec<Key> = specs(mac)
                .iter()
                .filter(|spec| spec.owner == owner)
                .map(listed)
                .collect();
            let keymap: Vec<Key> = keymap.iter().map(bound).collect();
            for key in &table {
                assert!(
                    keymap.contains(key),
                    "mac {mac}: the table lists {key:?}; {owner:?} does not bind it"
                );
            }
            for key in &keymap {
                assert!(
                    table.contains(key) || PLUMBING.contains(&key.1),
                    "mac {mac}: {owner:?} binds {key:?}; the table does not list it"
                );
            }
        }
    }
}

#[test]
fn every_row_has_keys_that_parse() {
    let mut labels = std::collections::HashSet::new();
    for row in SHORTCUTS {
        assert!(labels.insert(row.label), "{} listed twice", row.label);
        assert!(
            !row.keys(true).is_empty() || !row.keys(false).is_empty(),
            "{} shows no key on any platform",
            row.label
        );
        for mac in [true, false] {
            for spec in specs(mac) {
                Keystroke::parse(&spec.keys).expect("valid keystroke");
                if let Some(context) = spec.context {
                    KeyBindingContextPredicate::parse(context).expect("valid context");
                }
            }
        }
    }
}

#[test]
fn keys_read_as_each_platform_writes_them() {
    let cases = [
        ("mod-n", "⌘N", "Ctrl+N"),
        ("mod-shift-s", "⇧⌘S", "Ctrl+Shift+S"),
        ("mod-alt-shift-v", "⌥⇧⌘V", "Ctrl+Alt+Shift+V"),
        ("mod-\\", "⌘\\", "Ctrl+\\"),
        ("mod--", "⌘-", "Ctrl+-"),
        ("mod-/", "⌘/", "Ctrl+/"),
        ("?", "?", "?"),
        ("shift-enter", "⇧↩", "Shift+Enter"),
        ("mod-shift-backspace", "⇧⌘⌫", "Ctrl+Shift+Backspace"),
        ("shift-f6", "⇧F6", "Shift+F6"),
        ("ctrl-tab", "⌃Tab", "Ctrl+Tab"),
        ("escape", "Esc", "Esc"),
        ("pagedown", "Page Down", "Page Down"),
    ];
    for (key, mac, other) in cases {
        assert_eq!(display(&resolve(key, true), true), mac);
        assert_eq!(display(&resolve(key, false), false), other);
    }
    assert_eq!(display("ctrl-cmd-s", true), "⌃⌘S");
}

#[test]
fn the_palette_and_hints_read_keys_from_the_table() {
    let mod_ = if MAC { "⌘" } else { "Ctrl+" };
    assert_eq!(label(&workspace::New), Some(format!("{mod_}N")));
    assert_eq!(
        label(&shortcuts_ui::ShowShortcuts),
        Some(format!("{mod_}/"))
    );
    // Copy for AI has no key.
    assert_eq!(label(&pii::CopyForAi), None);
}
