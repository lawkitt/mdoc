//! Keyboard shortcuts: the one table of user-facing bindings (ADR 0073).
//!
//! Each row is what the shortcuts sheet shows: a label, its context group and
//! the keys for each platform. The palette's shortcut labels come from here
//! too. Each binding in a row names the module ([`Owner`]) whose `bind_keys`
//! registers it, so every module keeps its place in the keymap: GPUI breaks a
//! tie between equally deep bindings by registration order.
//!
//! Plumbing stays in its module and is not listed: Tab inside a control
//! group, moving inside menus, choosers and the palette list, and caret and
//! deletion basics in the editor and text fields. The invariant test in this
//! file names it.
//!
//! Keys use GPUI's syntax with one addition: `mod-` is ⌘ on macOS and Ctrl
//! on Windows and Linux.

use crate::{
    comparison_ui, document_view, markdown_search, menus,
    pii::ui as pii,
    search_session as find, settings_ui, spelling,
    workspace::{self, shortcuts_ui},
};
use gpui::{Action, KeyBinding, KeyBindingContextPredicate};

const MAC: bool = cfg!(target_os = "macos");

/// Where a shortcut works, as the sheet groups them, in the sheet's order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Everywhere,
    Editor,
    Popup,
    Panel,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Everywhere, Group::Editor, Group::Popup, Group::Panel];

    pub fn title(self) -> &'static str {
        match self {
            Group::Everywhere => "Everywhere",
            Group::Editor => "Editor",
            Group::Popup => "Popups, menus and dialogs",
            Group::Panel => "Panels and sidebar",
        }
    }
}

/// The `bind_keys` that registers a binding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Owner {
    /// `mdoc_editor`'s keys, through [`bind_crate`].
    Editor,
    /// Text fields (`markdown_search::bind_keys`).
    TextField,
    Pii,
    Cases,
    Shell,
    Settings,
    Dictionary,
    Ui,
    /// `gpui_pdf`'s keys, through [`bind_crate`].
    Pdf,
    Comparison,
    /// The find bar (`bind_markdown_search_keys`).
    Find,
    /// Window-wide keys (`bind_app_keys`).
    App,
    /// macOS menu keys (`menus::install`).
    Menus,
}

type Make = fn() -> Box<dyn Action>;

/// One action bound in one key context.
#[derive(Clone, Copy)]
pub struct Bind {
    owner: Owner,
    context: Option<&'static str>,
    action: Make,
    /// Shown and bound on macOS.
    mac: &'static [&'static str],
    /// Shown and bound on Windows and Linux.
    other: &'static [&'static str],
    /// Bound on every platform but not shown: alternates such as ⌘+ for
    /// Zoom in, or the editor's Ctrl keys on macOS.
    also: &'static [&'static str],
}

impl Bind {
    const fn new(
        owner: Owner,
        context: Option<&'static str>,
        keys: &'static [&'static str],
        action: Make,
    ) -> Self {
        Self {
            owner,
            context,
            action,
            mac: keys,
            other: keys,
            also: &[],
        }
    }
    const fn mac(self, keys: &'static [&'static str]) -> Self {
        Self { mac: keys, ..self }
    }
    const fn other(self, keys: &'static [&'static str]) -> Self {
        Self {
            other: keys,
            ..self
        }
    }
    const fn also(self, keys: &'static [&'static str]) -> Self {
        Self { also: keys, ..self }
    }
    fn shown(&self, mac: bool) -> &'static [&'static str] {
        if mac { self.mac } else { self.other }
    }
    /// Every key this binds on a platform, `mod-` resolved, shown keys first.
    fn keys(&self, mac: bool) -> Vec<String> {
        let mut keys = Vec::new();
        for key in self.shown(mac).iter().chain(self.also) {
            let key = resolve(key, mac);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }
}

/// A row of the shortcuts sheet.
pub struct Shortcut {
    pub label: &'static str,
    pub group: Group,
    binds: &'static [Bind],
}

impl Shortcut {
    /// The keys shown on a platform, in GPUI syntax with `mod-` resolved.
    pub fn keys(&self, mac: bool) -> Vec<String> {
        let mut keys = Vec::new();
        for bind in self.binds {
            for key in bind.shown(mac) {
                let key = resolve(key, mac);
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        keys
    }

    /// The keys shown on this platform, as the sheet and palette write them.
    pub fn key_labels(&self) -> Vec<String> {
        self.keys(MAC).iter().map(|key| display(key, MAC)).collect()
    }

    fn runs(&self, action: &dyn Action) -> bool {
        self.binds
            .iter()
            .any(|bind| (bind.action)().partial_eq(action))
    }
}

/// The rows shown on this platform, in table order.
pub fn rows() -> impl Iterator<Item = &'static Shortcut> {
    SHORTCUTS.iter().filter(|row| !row.keys(MAC).is_empty())
}

/// The first key of the first row that runs `action`, as the palette and
/// key hints write it.
pub fn label(action: &dyn Action) -> Option<String> {
    rows()
        .find(|row| row.runs(action))
        .and_then(|row| row.key_labels().into_iter().next())
}

/// One binding the table makes on a platform.
pub(crate) struct Spec {
    pub owner: Owner,
    pub keys: String,
    pub context: Option<&'static str>,
    pub action: Box<dyn Action>,
}

/// Every binding the table makes on a platform, in registration order.
pub(crate) fn specs(mac: bool) -> Vec<Spec> {
    let mut specs = Vec::new();
    for row in SHORTCUTS {
        for bind in row.binds {
            for keys in bind.keys(mac) {
                specs.push(Spec {
                    owner: bind.owner,
                    keys,
                    context: bind.context,
                    action: (bind.action)(),
                });
            }
        }
    }
    specs
}

impl Spec {
    fn binding(self) -> KeyBinding {
        let predicate = self.context.map(|context| {
            KeyBindingContextPredicate::parse(context)
                .expect("valid key context")
                .into()
        });
        KeyBinding::load(
            &self.keys,
            self.action,
            predicate,
            false,
            None,
            &gpui::DummyKeyboardMapper,
        )
        .expect("valid keystroke")
    }
}

/// The bindings `owner` registers on this platform.
pub fn bindings(owner: Owner) -> Vec<KeyBinding> {
    specs(MAC)
        .into_iter()
        .filter(|spec| spec.owner == owner)
        .map(Spec::binding)
        .collect()
}

/// Register a crate's own keymap with the table: the crate's plumbing as it
/// is, then its table rows. A crate binding the table does not make stays
/// bound, so the invariant test catches drift.
pub fn bind_crate(cx: &mut gpui::App, owner: Owner, keymap: Vec<KeyBinding>) {
    let table = bindings(owner);
    let mut all: Vec<_> = keymap
        .into_iter()
        .filter(|binding| !table.iter().any(|row| same(row, binding)))
        .collect();
    all.extend(table);
    cx.bind_keys(all);
}

fn same(a: &KeyBinding, b: &KeyBinding) -> bool {
    a.keystrokes() == b.keystrokes()
        && a.action().partial_eq(b.action())
        && a.predicate() == b.predicate()
}

/// `mod-` as ⌘ (`cmd-`) on macOS, Ctrl elsewhere.
fn resolve(key: &str, mac: bool) -> String {
    match key.strip_prefix("mod-") {
        Some(rest) => format!("{}-{rest}", if mac { "cmd" } else { "ctrl" }),
        None => key.to_string(),
    }
}

/// A key in GPUI syntax as people read it: `⇧⌘S` on macOS, `Ctrl+Shift+S`
/// on Windows and Linux.
pub fn display(key: &str, mac: bool) -> String {
    let (mut ctrl, mut alt, mut shift, mut cmd) = (false, false, false, false);
    let mut rest = key;
    loop {
        let flag = if let Some(r) = rest.strip_prefix("ctrl-") {
            rest = r;
            &mut ctrl
        } else if let Some(r) = rest.strip_prefix("alt-") {
            rest = r;
            &mut alt
        } else if let Some(r) = rest.strip_prefix("shift-") {
            rest = r;
            &mut shift
        } else if let Some(r) = rest.strip_prefix("cmd-") {
            rest = r;
            &mut cmd
        } else {
            break;
        };
        *flag = true;
    }
    let name = match (rest, mac) {
        ("enter", true) => "↩".to_string(),
        ("enter", false) => "Enter".to_string(),
        ("backspace", true) => "⌫".to_string(),
        ("backspace", false) => "Backspace".to_string(),
        ("delete", true) => "⌦".to_string(),
        ("delete", false) => "Delete".to_string(),
        ("escape", _) => "Esc".to_string(),
        ("tab", _) => "Tab".to_string(),
        ("space", _) => "Space".to_string(),
        ("up", _) => "↑".to_string(),
        ("down", _) => "↓".to_string(),
        ("left", _) => "←".to_string(),
        ("right", _) => "→".to_string(),
        ("home", _) => "Home".to_string(),
        ("end", _) => "End".to_string(),
        (key, _) if key.len() > 1 && key.starts_with('f') => key.to_uppercase(),
        (key, _) => key.to_uppercase(),
    };
    if mac {
        let mut text = String::new();
        for (on, symbol) in [(ctrl, "⌃"), (alt, "⌥"), (shift, "⇧"), (cmd, "⌘")] {
            if on {
                text.push_str(symbol);
            }
        }
        text + &name
    } else {
        let mut parts = Vec::new();
        for (on, word) in [(ctrl, "Ctrl"), (alt, "Alt"), (shift, "Shift"), (cmd, "Win")] {
            if on {
                parts.push(word.to_string());
            }
        }
        parts.push(name);
        parts.join("+")
    }
}

/// Chrome outside the editor and text fields: where `?` opens the sheet and
/// Undo reaches the active document.
pub(crate) const CHROME: &str = "Workspace && !Editor && !TextInput";
const EDITOR: Option<&str> = Some("Editor");
const FIND: Option<&str> = Some("MarkdownSearch");
const REPLACE: Option<&str> = Some("MarkdownReplace");

/// A binding of `$action` for `$owner` in `$context`.
macro_rules! on {
    ($owner:ident, $context:expr, $keys:expr, $action:expr) => {
        Bind::new(Owner::$owner, $context, $keys, || Box::new($action))
    };
}

const fn row(group: Group, label: &'static str, binds: &'static [Bind]) -> Shortcut {
    Shortcut {
        label,
        group,
        binds,
    }
}

use Group::{Editor, Everywhere, Panel, Popup};

/// Every user-facing binding. The sheet shows rows by group in this order.
/// Panel rows come before Popup rows here only to keep the review keys'
/// registration order (see the module comment).
pub static SHORTCUTS: &[Shortcut] = &[
    // ---- Everywhere ---------------------------------------------------------
    row(
        Everywhere,
        "New tab",
        &[on!(App, None, &["mod-n"], workspace::New)],
    ),
    row(
        Everywhere,
        "Open files…",
        &[on!(App, None, &["mod-o"], workspace::Open)],
    ),
    row(
        Everywhere,
        "Convert to Markdown",
        &[on!(App, None, &["mod-shift-i"], workspace::Import)],
    ),
    row(
        Everywhere,
        "Save",
        &[on!(App, None, &["mod-s"], document_view::Save)],
    ),
    row(
        Everywhere,
        "Save as…",
        &[on!(App, None, &["mod-shift-s"], document_view::SaveAs)],
    ),
    row(
        Everywhere,
        "Close tab",
        &[on!(App, None, &["mod-w"], workspace::Close)],
    ),
    row(
        Everywhere,
        "Command palette",
        &[on!(Shell, None, &["mod-k"], workspace::OpenPalette)],
    ),
    row(
        Everywhere,
        "Keyboard shortcuts",
        &[
            on!(Shell, None, &["mod-/"], shortcuts_ui::ShowShortcuts),
            on!(Shell, Some(CHROME), &["?"], shortcuts_ui::ShowShortcuts),
        ],
    ),
    row(
        Everywhere,
        "Home",
        &[on!(Shell, None, &["mod-shift-h"], workspace::ShowHome)],
    ),
    row(
        Everywhere,
        "Settings",
        &[on!(App, None, &["mod-,"], workspace::Settings)],
    ),
    row(
        Everywhere,
        "Next tab",
        &[on!(App, None, &["ctrl-tab"], workspace::NextTab)],
    ),
    row(
        Everywhere,
        "Previous tab",
        &[on!(App, None, &["ctrl-shift-tab"], workspace::PreviousTab)],
    ),
    // The macOS convention for View › Toggle Sidebar.
    row(
        Everywhere,
        "Toggle sidebar",
        &[on!(App, None, &[], workspace::ToggleSidebar).mac(&["ctrl-cmd-s"])],
    ),
    // Keyboard reach (ADR 0062).
    row(
        Everywhere,
        "Next region",
        &[on!(Ui, None, &["f6"], crate::ui::NextRegion)],
    ),
    row(
        Everywhere,
        "Previous region",
        &[on!(Ui, None, &["shift-f6"], crate::ui::PreviousRegion)],
    ),
    row(
        Everywhere,
        "Next control",
        &[on!(Ui, Some("Workspace"), &["tab"], crate::ui::NextControl)],
    ),
    row(
        Everywhere,
        "Previous control",
        &[on!(
            Ui,
            Some("Workspace"),
            &["shift-tab"],
            crate::ui::PreviousControl
        )],
    ),
    row(
        Everywhere,
        "Back to the editor from the sidebar or toolbar",
        &[on!(
            Ui,
            Some("SidebarRegion || ToolbarRegion"),
            &["escape"],
            crate::ui::FocusEditor
        )],
    ),
    row(
        Everywhere,
        "Close notices",
        &[on!(
            Shell,
            Some("Workspace"),
            &["escape"],
            workspace::CloseNotices
        )],
    ),
    // One document history: the editor, the review popup and panel, the find
    // bar and the chrome all undo the document (ADR 0033).
    row(
        Everywhere,
        "Undo",
        &[
            on!(Editor, EDITOR, &["mod-z"], mdoc_editor::Undo).also(&["cmd-z", "ctrl-z"]),
            on!(Pii, Some("IdentityPanel"), &["mod-z"], mdoc_editor::Undo)
                .also(&["cmd-z", "ctrl-z"]),
            on!(Pii, Some(pii::POPUP_UNDO), &["mod-z"], mdoc_editor::Undo)
                .also(&["cmd-z", "ctrl-z"]),
            on!(Find, FIND, &["mod-z"], mdoc_editor::Undo),
            on!(Find, REPLACE, &["mod-z"], mdoc_editor::Undo),
            on!(App, Some(CHROME), &["mod-z"], mdoc_editor::Undo),
        ],
    ),
    row(
        Everywhere,
        "Redo",
        &[
            on!(Editor, EDITOR, &[], mdoc_editor::Redo)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"])
                .also(&["cmd-shift-z", "ctrl-shift-z", "ctrl-y"]),
            on!(Pii, Some("IdentityPanel"), &[], mdoc_editor::Redo)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"])
                .also(&["cmd-shift-z", "ctrl-shift-z", "ctrl-y"]),
            on!(Pii, Some(pii::POPUP_UNDO), &[], mdoc_editor::Redo)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"])
                .also(&["cmd-shift-z", "ctrl-shift-z", "ctrl-y"]),
            on!(Find, FIND, &[], mdoc_editor::Redo)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"]),
            on!(Find, REPLACE, &[], mdoc_editor::Redo)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"]),
            on!(App, Some(CHROME), &[], mdoc_editor::Redo)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"]),
        ],
    ),
    // Layouts where plus needs Shift (US: Shift + =) report the Shift.
    row(
        Everywhere,
        "Zoom in",
        &[on!(App, None, &["mod-="], document_view::ZoomIn).also(&[
            "mod-+",
            "mod-shift-=",
            "mod-shift-+",
        ])],
    ),
    row(
        Everywhere,
        "Zoom out",
        &[on!(App, None, &["mod--"], document_view::ZoomOut)],
    ),
    row(
        Everywhere,
        "Actual size",
        &[on!(App, None, &["mod-0"], document_view::ZoomReset)],
    ),
    row(
        Everywhere,
        "Before / After",
        &[on!(App, None, &["mod-\\"], document_view::ToggleCompare)],
    ),
    row(
        Everywhere,
        "Pseudonymize",
        &[on!(Pii, None, &["mod-shift-p"], pii::Pseudonymize)],
    ),
    row(
        Everywhere,
        "Apply all replacements",
        &[on!(Pii, None, &["mod-enter"], pii::PiiApplyAll)],
    ),
    row(
        Everywhere,
        "Paste AI response",
        &[on!(
            App,
            None,
            &["mod-alt-shift-v"],
            workspace::PasteAiResponse
        )],
    ),
    row(
        Everywhere,
        "Pseudonymize clipboard",
        &[on!(
            App,
            None,
            &["mod-alt-shift-c"],
            workspace::PseudonymizeClipboard
        )],
    ),
    row(
        Everywhere,
        "Hide mdoc",
        &[on!(Menus, None, &[], menus::HideApp).mac(&["cmd-h"])],
    ),
    row(
        Everywhere,
        "Hide others",
        &[on!(Menus, None, &[], menus::HideOtherApps).mac(&["cmd-alt-h"])],
    ),
    row(
        Everywhere,
        "Minimize",
        &[on!(Menus, None, &[], menus::MinimizeWindow).mac(&["cmd-m"])],
    ),
    row(
        Everywhere,
        "Quit",
        &[on!(App, None, &["mod-q"], workspace::Quit)],
    ),
    // ---- Editor -------------------------------------------------------------
    row(
        Editor,
        "New paragraph",
        &[on!(Editor, EDITOR, &["enter"], mdoc_editor::Newline)],
    ),
    row(
        Editor,
        "Line break",
        &[on!(
            Editor,
            EDITOR,
            &["shift-enter"],
            mdoc_editor::LineBreak
        )],
    ),
    row(
        Editor,
        "Indent",
        &[on!(Editor, EDITOR, &["tab"], mdoc_editor::Indent)],
    ),
    row(
        Editor,
        "Outdent",
        &[on!(Editor, EDITOR, &["shift-tab"], mdoc_editor::Outdent)],
    ),
    row(
        Editor,
        "Bold",
        &[on!(Editor, EDITOR, &["mod-b"], mdoc_editor::Bold).also(&["cmd-b", "ctrl-b"])],
    ),
    row(
        Editor,
        "Italic",
        &[on!(Editor, EDITOR, &["mod-i"], mdoc_editor::Italic).also(&["cmd-i", "ctrl-i"])],
    ),
    row(
        Editor,
        "Underline",
        &[on!(Editor, EDITOR, &["mod-u"], mdoc_editor::Underline).also(&["cmd-u", "ctrl-u"])],
    ),
    row(
        Editor,
        "Strikethrough",
        &[on!(Editor, EDITOR, &["mod-shift-x"], mdoc_editor::Strike)
            .also(&["cmd-shift-x", "ctrl-shift-x"])],
    ),
    row(
        Editor,
        "Code",
        &[on!(Editor, EDITOR, &["mod-e"], mdoc_editor::Code).also(&["cmd-e", "ctrl-e"])],
    ),
    row(
        Editor,
        "Select all",
        &[
            on!(Editor, EDITOR, &["mod-a"], mdoc_editor::SelectAll).also(&["cmd-a", "ctrl-a"]),
            on!(
                TextField,
                Some(markdown_search::TEXT_FIELDS),
                &["mod-a"],
                markdown_search::SelectAll
            )
            .also(&["cmd-a", "ctrl-a"]),
        ],
    ),
    row(
        Editor,
        "Copy",
        &[
            on!(Editor, EDITOR, &["mod-c"], mdoc_editor::Copy).also(&["cmd-c", "ctrl-c"]),
            on!(
                TextField,
                Some(markdown_search::TEXT_FIELDS),
                &["mod-c"],
                markdown_search::Copy
            )
            .also(&["cmd-c", "ctrl-c"]),
        ],
    ),
    row(
        Editor,
        "Cut",
        &[
            on!(Editor, EDITOR, &["mod-x"], mdoc_editor::Cut).also(&["cmd-x", "ctrl-x"]),
            on!(
                TextField,
                Some(markdown_search::TEXT_FIELDS),
                &["mod-x"],
                markdown_search::Cut
            )
            .also(&["cmd-x", "ctrl-x"]),
        ],
    ),
    row(
        Editor,
        "Paste",
        &[
            on!(Editor, EDITOR, &["mod-v"], mdoc_editor::Paste).also(&["cmd-v", "ctrl-v"]),
            on!(
                TextField,
                Some(markdown_search::TEXT_FIELDS),
                &["mod-v"],
                markdown_search::Paste
            )
            .also(&["cmd-v", "ctrl-v"]),
        ],
    ),
    // Caret moves (ADR 0061): macOS keeps ⌥ for words and ⌘ for line and
    // document ends; Windows and Linux use Ctrl and Home / End.
    row(
        Editor,
        "Previous word",
        &[on!(Editor, EDITOR, &[], mdoc_editor::WordLeft)
            .mac(&["alt-left"])
            .other(&["ctrl-left"])
            .also(&["alt-left"])],
    ),
    row(
        Editor,
        "Next word",
        &[
            on!(Editor, EDITOR, &[], mdoc_editor::WordRight)
                .mac(&["alt-right"])
                .also(&["alt-right"]),
            on!(Editor, EDITOR, &[], mdoc_editor::NextWordStart).other(&["ctrl-right"]),
        ],
    ),
    row(
        Editor,
        "Select to previous word",
        &[on!(Editor, EDITOR, &[], mdoc_editor::SelectWordLeft)
            .mac(&["alt-shift-left"])
            .other(&["ctrl-shift-left"])
            .also(&["alt-shift-left"])],
    ),
    row(
        Editor,
        "Select to next word",
        &[
            on!(Editor, EDITOR, &[], mdoc_editor::SelectWordRight)
                .mac(&["alt-shift-right"])
                .also(&["alt-shift-right"]),
            on!(Editor, EDITOR, &[], mdoc_editor::SelectToNextWordStart)
                .other(&["ctrl-shift-right"]),
        ],
    ),
    row(
        Editor,
        "Line start",
        &[on!(Editor, EDITOR, &[], mdoc_editor::LineStart).mac(&["cmd-left"])],
    ),
    row(
        Editor,
        "Line end",
        &[on!(Editor, EDITOR, &[], mdoc_editor::LineEnd).mac(&["cmd-right"])],
    ),
    row(
        Editor,
        "Select to line start",
        &[on!(Editor, EDITOR, &[], mdoc_editor::SelectToLineStart).mac(&["cmd-shift-left"])],
    ),
    row(
        Editor,
        "Select to line end",
        &[on!(Editor, EDITOR, &[], mdoc_editor::SelectToLineEnd).mac(&["cmd-shift-right"])],
    ),
    row(
        Editor,
        "Document start",
        &[on!(Editor, EDITOR, &[], mdoc_editor::DocumentStart)
            .mac(&["cmd-up"])
            .other(&["ctrl-home"])],
    ),
    row(
        Editor,
        "Document end",
        &[on!(Editor, EDITOR, &[], mdoc_editor::DocumentEnd)
            .mac(&["cmd-down"])
            .other(&["ctrl-end"])],
    ),
    row(
        Editor,
        "Select to document start",
        &[on!(Editor, EDITOR, &[], mdoc_editor::SelectToDocumentStart)
            .mac(&["cmd-shift-up"])
            .other(&["ctrl-shift-home"])],
    ),
    row(
        Editor,
        "Select to document end",
        &[on!(Editor, EDITOR, &[], mdoc_editor::SelectToDocumentEnd)
            .mac(&["cmd-shift-down"])
            .other(&["ctrl-shift-end"])],
    ),
    row(
        Editor,
        "Delete previous word",
        &[on!(Editor, EDITOR, &[], mdoc_editor::DeleteWordLeft)
            .mac(&["alt-backspace"])
            .other(&["ctrl-backspace"])],
    ),
    row(
        Editor,
        "Delete next word",
        &[
            on!(Editor, EDITOR, &[], mdoc_editor::DeleteToWordEnd).mac(&["alt-delete"]),
            on!(Editor, EDITOR, &[], mdoc_editor::DeleteWordRight).other(&["ctrl-delete"]),
        ],
    ),
    row(
        Editor,
        "Delete to line start",
        &[on!(Editor, EDITOR, &[], mdoc_editor::DeleteToLineStart).mac(&["cmd-backspace"])],
    ),
    // Find and Replace keys follow Zed's defaults (ADR 0063).
    row(
        Editor,
        "Find",
        &[
            on!(Find, EDITOR, &["mod-f"], find::FindMarkdown),
            on!(Find, None, &["mod-f"], find::FindMarkdown),
            on!(Find, FIND, &["mod-f"], find::FindMarkdown),
            on!(Find, REPLACE, &["mod-f"], find::FindMarkdown),
        ],
    ),
    row(
        Editor,
        "Find and replace",
        &[
            on!(Find, EDITOR, &[], find::FindReplaceMarkdown)
                .mac(&["cmd-alt-f"])
                .other(&["ctrl-h"]),
            on!(Find, None, &[], find::FindReplaceMarkdown)
                .mac(&["cmd-alt-f"])
                .other(&["ctrl-h"]),
            on!(Find, FIND, &[], find::FindReplaceMarkdown)
                .mac(&["cmd-alt-f"])
                .other(&["ctrl-h"]),
            on!(Find, REPLACE, &[], find::FindReplaceMarkdown)
                .mac(&["cmd-alt-f"])
                .other(&["ctrl-h"]),
        ],
    ),
    row(
        Editor,
        "Find next",
        &[
            on!(Find, EDITOR, &["mod-g"], find::FindNextMarkdown),
            on!(Find, FIND, &["mod-g"], find::FindNextMarkdown),
            on!(Find, REPLACE, &["mod-g"], find::FindNextMarkdown),
        ],
    ),
    row(
        Editor,
        "Find previous",
        &[
            on!(Find, EDITOR, &["mod-shift-g"], find::FindPreviousMarkdown),
            on!(Find, FIND, &["mod-shift-g"], find::FindPreviousMarkdown),
            on!(Find, REPLACE, &["mod-shift-g"], find::FindPreviousMarkdown),
        ],
    ),
    row(
        Editor,
        "Next match, in the find field",
        &[on!(Find, FIND, &["enter"], find::FindNextMarkdown)],
    ),
    row(
        Editor,
        "Previous match, in the find field",
        &[on!(
            Find,
            FIND,
            &["shift-enter"],
            find::FindPreviousMarkdown
        )],
    ),
    row(
        Editor,
        "Replace and find next, in the replace field",
        &[on!(Find, REPLACE, &["enter"], find::ReplaceNextMarkdown)],
    ),
    row(
        Editor,
        "Replace all",
        &[
            on!(Find, FIND, &[], find::ReplaceAllMarkdown)
                .mac(&["cmd-enter"])
                .other(&["ctrl-alt-enter"]),
            on!(Find, REPLACE, &[], find::ReplaceAllMarkdown)
                .mac(&["cmd-enter"])
                .other(&["ctrl-alt-enter"]),
        ],
    ),
    row(
        Editor,
        "Match case",
        &[
            on!(Find, FIND, &[], find::ToggleMarkdownMatchCase)
                .mac(&["cmd-alt-c"])
                .other(&["alt-c"]),
            on!(Find, REPLACE, &[], find::ToggleMarkdownMatchCase)
                .mac(&["cmd-alt-c"])
                .other(&["alt-c"]),
        ],
    ),
    row(
        Editor,
        "Whole word",
        &[
            on!(Find, FIND, &[], find::ToggleMarkdownWholeWord)
                .mac(&["cmd-alt-w"])
                .other(&["alt-w"]),
            on!(Find, REPLACE, &[], find::ToggleMarkdownWholeWord)
                .mac(&["cmd-alt-w"])
                .other(&["alt-w"]),
        ],
    ),
    row(
        Editor,
        "Close find",
        &[
            on!(Find, FIND, &["escape"], find::CloseMarkdownSearch),
            on!(Find, REPLACE, &["escape"], find::CloseMarkdownSearch),
        ],
    ),
    row(
        Editor,
        "Next misspelling",
        &[on!(App, None, &["mod-;"], spelling::NextMisspelling)],
    ),
    row(
        Editor,
        "Previous misspelling",
        &[on!(
            App,
            None,
            &["mod-shift-;"],
            spelling::PreviousMisspelling
        )],
    ),
    row(
        Editor,
        "Review the proposal at the caret",
        &[on!(Pii, EDITOR, &["alt-enter"], pii::PiiReviewCandidate)],
    ),
    row(
        Editor,
        "Next proposal",
        &[on!(Pii, EDITOR, &["alt-down"], pii::PiiNextCandidate)],
    ),
    row(
        Editor,
        "Previous proposal",
        &[on!(Pii, EDITOR, &["alt-up"], pii::PiiPreviousCandidate)],
    ),
    row(
        Editor,
        "Propose the selection",
        &[on!(Pii, EDITOR, &["mod-alt-p"], pii::PiiAddCandidate)],
    ),
    // Esc leaves before/after, also from inside its read-only columns.
    row(
        Editor,
        "Leave Before / After",
        &[
            on!(Pii, Some("CompareView"), &["escape"], pii::CloseCompare),
            on!(
                Pii,
                Some("CompareView > Editor"),
                &["escape"],
                pii::CloseCompare
            ),
        ],
    ),
    // ---- Panels and sidebar -------------------------------------------------
    // The sidebar's tab list is one Tab stop; arrows move inside it (ADR 0062).
    row(
        Panel,
        "Sidebar: previous tab",
        &[on!(
            Ui,
            Some("SidebarTab"),
            &["up"],
            crate::ui::SidebarPrevious
        )],
    ),
    row(
        Panel,
        "Sidebar: next tab",
        &[on!(
            Ui,
            Some("SidebarTab"),
            &["down"],
            crate::ui::SidebarNext
        )],
    ),
    row(
        Panel,
        "Sidebar: close the tab",
        &[on!(
            Ui,
            Some("SidebarTab"),
            &["backspace"],
            crate::ui::SidebarCloseTab
        )
        .also(&["delete"])],
    ),
    row(
        Panel,
        "Sidebar: the tab's case actions",
        &[on!(
            Ui,
            Some("SidebarTab"),
            &["shift-f10"],
            crate::ui::SidebarMenu
        )],
    ),
    // The Replacements checklist (ADR 0033, 0048).
    row(
        Panel,
        "Replacements: previous row",
        &[on!(Pii, Some(pii::LIST), &["up"], pii::PanelUp)],
    ),
    row(
        Panel,
        "Replacements: next row",
        &[on!(Pii, Some(pii::LIST), &["down"], pii::PanelDown)],
    ),
    row(
        Panel,
        "Replacements: first row",
        &[on!(Pii, Some(pii::LIST), &["mod-up"], pii::PanelFirst)],
    ),
    row(
        Panel,
        "Replacements: last row",
        &[on!(Pii, Some(pii::LIST), &["mod-down"], pii::PanelLast)],
    ),
    row(
        Panel,
        "Replacements: move to the group above",
        &[on!(Pii, Some(pii::LIST), &["alt-up"], pii::PanelMoveUp)],
    ),
    row(
        Panel,
        "Replacements: move to the group below",
        &[on!(Pii, Some(pii::LIST), &["alt-down"], pii::PanelMoveDown)],
    ),
    row(
        Panel,
        "Replacements: back to the filter, then close",
        &[on!(Pii, Some("IdentityPanel"), &["escape"], pii::PanelEscape).also(&["alt-escape"])],
    ),
    row(
        Panel,
        "Replacements: tick or untick",
        &[on!(Pii, Some(pii::LIST), &["space"], pii::PanelToggle)],
    ),
    row(
        Panel,
        "Replacements: go to the mention",
        &[on!(Pii, Some(pii::LIST), &["enter"], pii::PanelOpen)],
    ),
    row(
        Panel,
        "Replacements: show mentions",
        &[on!(Pii, Some(pii::LIST), &["right"], pii::PanelExpand)],
    ),
    row(
        Panel,
        "Replacements: hide mentions",
        &[on!(Pii, Some(pii::LIST), &["left"], pii::PanelCollapse)],
    ),
    row(
        Panel,
        "Replacements: keep the original",
        &[on!(Pii, Some(pii::LIST), &["backspace"], pii::PanelKeep).also(&["delete"])],
    ),
    row(
        Panel,
        "Replacements: from the filter to the list",
        &[on!(
            Pii,
            Some("ReplacementSearch"),
            &["down"],
            pii::PanelEnterList
        )],
    ),
    // ---- Popups, menus and dialogs ------------------------------------------
    // The mention popup (ADR 0055).
    row(
        Popup,
        "Apply and go to the next proposal",
        &[
            on!(Pii, Some(pii::POPUP), &["enter"], pii::ApplyThis),
            // Scope segments keep Space for selection, but Enter applies in
            // the chosen scope instead of pressing the segment again (#177).
            on!(Pii, Some("PiiScope"), &["enter"], pii::ApplyThis),
        ],
    ),
    row(
        Popup,
        "Tick or untick Replace",
        &[on!(Pii, Some(pii::POPUP), &["space"], pii::PanelKeep)],
    ),
    row(
        Popup,
        "Keep the original here",
        &[on!(Pii, Some(pii::POPUP), &["backspace"], pii::KeepThis).also(&["delete"])],
    ),
    row(
        Popup,
        "Keep this wording everywhere",
        &[on!(Pii, Some(pii::POPUP), &["mod-backspace"], pii::KeepSame).also(&["mod-delete"])],
    ),
    row(
        Popup,
        "Keep the whole entity",
        &[on!(
            Pii,
            Some(pii::POPUP),
            &["mod-shift-backspace"],
            pii::KeepAll
        )
        .also(&["mod-shift-delete"])],
    ),
    row(
        Popup,
        "Previous mention",
        &[on!(Pii, Some(pii::STEP), &["left"], pii::PopupPreviousMention).also(&["up"])],
    ),
    row(
        Popup,
        "Next mention",
        &[on!(Pii, Some(pii::STEP), &["right"], pii::PopupNextMention).also(&["down"])],
    ),
    row(
        Popup,
        "First mention",
        &[on!(Pii, Some(pii::STEP), &["mod-up"], pii::PopupFirstMention).also(&["mod-left"])],
    ),
    row(
        Popup,
        "Last mention",
        &[on!(Pii, Some(pii::STEP), &["mod-down"], pii::PopupLastMention).also(&["mod-right"])],
    ),
    // In the alias field Enter confirms the draft (ADR 0033).
    row(
        Popup,
        "Confirm the alias",
        &[on!(
            Pii,
            Some("PseudonymReplacement"),
            &["enter"],
            pii::PiiConfirm
        )],
    ),
    row(
        Popup,
        "Close the popup",
        &[on!(
            Pii,
            Some("PseudonymReview"),
            &["escape"],
            pii::PiiClosePopup
        )],
    ),
    row(
        Popup,
        "Close a menu",
        &[
            on!(Ui, Some("UiMenu"), &["escape"], crate::ui::CloseMenu),
            on!(Pdf, Some("PdfMenu"), &["escape"], gpui_pdf::MenuClose),
        ],
    ),
    row(
        Popup,
        "Close the command palette",
        &[on!(
            Shell,
            Some(workspace::PALETTE),
            &["escape"],
            workspace::PaletteClose
        )],
    ),
    row(
        Popup,
        "Close keyboard shortcuts",
        &[on!(
            Shell,
            Some(shortcuts_ui::SHEET),
            &["escape"],
            shortcuts_ui::CloseShortcuts
        )],
    ),
    row(
        Popup,
        "Close Settings",
        &[on!(
            Settings,
            Some("ModelSettings"),
            &["escape"],
            settings_ui::CloseSettings
        )],
    ),
    row(
        Popup,
        "Add the word to the dictionary",
        &[on!(
            Dictionary,
            Some("DictionarySearch"),
            &["enter"],
            settings_ui::dictionary::AddDictionaryWord
        )],
    ),
    row(
        Popup,
        "Save the case name",
        &[on!(
            Cases,
            Some("CaseName"),
            &["enter"],
            workspace::ConfirmCaseName
        )],
    ),
    row(
        Popup,
        "Cancel renaming the case",
        &[on!(
            Cases,
            Some("CaseName"),
            &["escape"],
            workspace::CancelCaseName
        )],
    ),
    row(
        Popup,
        "Close the model comparison",
        &[on!(
            Comparison,
            Some("ModelComparison"),
            &["escape"],
            comparison_ui::CloseComparison
        )],
    ),
];

#[cfg(test)]
mod tests;
