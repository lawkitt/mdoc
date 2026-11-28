//! Keyboard shortcuts: the one table of user-facing bindings.
//!
//! Each row is what the shortcuts sheet shows: a label, its context group and
//! the keys for each platform. The palette's shortcut labels come from here
//! too. A command's row takes its name and action from the command table. Each binding in a row names the module ([`Owner`]) whose `bind_keys`
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
    commands::{table, text},
    document::comparison_ui,
    document::markdown_search,
    document::search_session as find,
    document::spelling,
    review as pii,
    shelf::shelf_ui,
    ui::confirm_dialog,
    workspace::{self, palette::shortcuts_ui},
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
            Group::Everywhere => text::EVERYWHERE,
            Group::Editor => text::EDITOR,
            Group::Popup => text::POPUPS,
            Group::Panel => text::PANELS,
        }
    }
}

/// The `bind_keys` that registers a binding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Owner {
    /// `mdoc_editor`'s keys, through [`bind_crate`].
    Editor,
    /// Text fields (`document::markdown_search::bind_keys`).
    TextField,
    Pii,
    Projects,
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
    /// The confirm dialog (`confirm_dialog::bind_keys`).
    Dialog,
    /// The menu bar shelf (`shelf_ui::bind_keys`).
    Shelf,
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

/// A tooltip that names its key when the table gives `action` one on this
/// platform: "Back to Home (⌘[)". The accessible name stays the bare label.
pub fn tip(label: impl Into<String>, action: &dyn Action) -> String {
    let label = label.into();
    match self::label(action) {
        Some(key) => format!("{label} ({key})"),
        None => label,
    }
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
    fn binding(self, cx: &gpui::App) -> KeyBinding {
        let predicate = self.context.map(|context| {
            KeyBindingContextPredicate::parse(context)
                .expect("valid key context")
                .into()
        });
        KeyBinding::load(&self.keys, self.action, predicate, false, None, cx)
            .expect("valid keystroke")
    }
}

/// The bindings `owner` registers on this platform.
pub fn bindings(owner: Owner, cx: &gpui::App) -> Vec<KeyBinding> {
    specs(MAC)
        .into_iter()
        .filter(|spec| spec.owner == owner)
        .map(|spec| spec.binding(cx))
        .collect()
}

/// Register a crate's own keymap with the table: the crate's plumbing as it
/// is, then its table rows. A crate binding the table does not make stays
/// bound, so the invariant test catches drift.
pub fn bind_crate(cx: &mut gpui::App, owner: Owner, keymap: Vec<KeyBinding>) {
    let table = bindings(owner, cx);
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
        ("enter", false) => text::KEY_ENTER.to_string(),
        ("backspace", true) => "⌫".to_string(),
        ("backspace", false) => text::KEY_BACKSPACE.to_string(),
        ("delete", true) => "⌦".to_string(),
        ("delete", false) => text::KEY_DELETE.to_string(),
        ("escape", _) => text::KEY_ESC.to_string(),
        ("menu", _) => text::KEY_MENU.to_string(),
        ("tab", _) => text::KEY_TAB.to_string(),
        ("space", _) => text::KEY_SPACE.to_string(),
        ("up", _) => "↑".to_string(),
        ("down", _) => "↓".to_string(),
        ("left", _) => "←".to_string(),
        ("right", _) => "→".to_string(),
        ("home", _) => text::KEY_HOME.to_string(),
        ("pageup", _) => text::KEY_PAGE_UP.to_string(),
        ("pagedown", _) => text::KEY_PAGE_DOWN.to_string(),
        ("end", _) => text::KEY_END.to_string(),
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
        for (on, word) in [
            (ctrl, text::KEY_CTRL),
            (alt, text::KEY_ALT),
            (shift, text::KEY_SHIFT),
            (cmd, text::KEY_WIN),
        ] {
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

/// A binding of command `$command`'s action for `$owner` in `$context`.
macro_rules! cmd {
    ($owner:ident, $context:expr, $keys:expr, $command:ident) => {
        Bind::new(Owner::$owner, $context, $keys, table::$command.make)
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
        table::NEW.sheet,
        &[cmd!(App, None, &["mod-n"], NEW)],
    ),
    row(
        Everywhere,
        table::NEW_PROJECT.sheet,
        &[cmd!(App, None, &["mod-shift-n"], NEW_PROJECT)],
    ),
    row(
        Everywhere,
        table::OPEN.sheet,
        &[cmd!(App, None, &["mod-o"], OPEN)],
    ),
    row(
        Everywhere,
        table::CONVERT_TO_MARKDOWN.sheet,
        &[cmd!(App, None, &["mod-shift-i"], CONVERT_TO_MARKDOWN)],
    ),
    row(
        Everywhere,
        table::SAVE.sheet,
        &[cmd!(App, None, &["mod-s"], SAVE)],
    ),
    row(
        Everywhere,
        table::EXPORT_MARKDOWN.sheet,
        &[cmd!(App, None, &["mod-shift-s"], EXPORT_MARKDOWN)],
    ),
    row(
        Everywhere,
        table::CLOSE.sheet,
        &[cmd!(App, None, &["mod-w"], CLOSE)],
    ),
    row(
        Everywhere,
        table::COMMAND_PALETTE.sheet,
        &[cmd!(Shell, None, &["mod-k"], COMMAND_PALETTE)],
    ),
    row(
        Everywhere,
        table::KEYBOARD_SHORTCUTS.sheet,
        &[
            cmd!(Shell, None, &["mod-/"], KEYBOARD_SHORTCUTS),
            cmd!(Shell, Some(CHROME), &["?"], KEYBOARD_SHORTCUTS),
        ],
    ),
    row(
        Everywhere,
        table::HOME.sheet,
        &[cmd!(Shell, None, &["mod-shift-h"], HOME)],
    ),
    row(
        Everywhere,
        text::BACK,
        &[
            on!(Shell, Some("Workspace"), &[], workspace::Back).mac(&["cmd-["]),
            on!(Shell, Some(CHROME), &[], workspace::Back).other(&["alt-left"]),
        ],
    ),
    row(
        Everywhere,
        text::FORWARD,
        &[
            on!(Shell, Some("Workspace"), &[], workspace::Forward).mac(&["cmd-]"]),
            on!(Shell, Some(CHROME), &[], workspace::Forward).other(&["alt-right"]),
        ],
    ),
    row(
        Everywhere,
        table::SETTINGS.sheet,
        &[cmd!(App, None, &["mod-,"], SETTINGS)],
    ),
    row(
        Everywhere,
        table::SWITCH_DOCUMENT.sheet,
        &[cmd!(App, None, &["ctrl-tab"], SWITCH_DOCUMENT)],
    ),
    row(
        Everywhere,
        table::SWITCH_DOCUMENT_BACK.sheet,
        &[cmd!(App, None, &["ctrl-shift-tab"], SWITCH_DOCUMENT_BACK)],
    ),
    // The macOS convention for View › Toggle Sidebar.
    row(
        Everywhere,
        table::TOGGLE_SIDEBAR.sheet,
        &[cmd!(App, None, &[], TOGGLE_SIDEBAR).mac(&["ctrl-cmd-s"])],
    ),
    // Keyboard reach.
    row(
        Everywhere,
        text::NEXT_REGION,
        &[on!(Ui, None, &["f6"], crate::ui::NextRegion)],
    ),
    row(
        Everywhere,
        text::PREVIOUS_REGION,
        &[on!(Ui, None, &["shift-f6"], crate::ui::PreviousRegion)],
    ),
    row(
        Everywhere,
        text::NEXT_CONTROL,
        &[on!(Ui, Some("Workspace"), &["tab"], crate::ui::NextControl)],
    ),
    row(
        Everywhere,
        text::PREVIOUS_CONTROL,
        &[on!(
            Ui,
            Some("Workspace"),
            &["shift-tab"],
            crate::ui::PreviousControl
        )],
    ),
    row(
        Everywhere,
        text::BACK_TO_THE_EDITOR_FROM_THE_SIDEBAR_OR_TOOLBAR,
        &[on!(
            Ui,
            Some("SidebarRegion || ToolbarRegion"),
            &["escape"],
            crate::ui::FocusEditor
        )],
    ),
    row(
        Everywhere,
        text::CLOSE_NOTICES,
        &[on!(
            Shell,
            Some("Workspace"),
            &["escape"],
            workspace::CloseNotices
        )],
    ),
    // One document history: the editor, the review popup and panel, the find
    // bar and the chrome all undo the document.
    row(
        Everywhere,
        table::UNDO.sheet,
        &[
            cmd!(Editor, EDITOR, &["mod-z"], UNDO).also(&["cmd-z", "ctrl-z"]),
            cmd!(Pii, Some("IdentityPanel"), &["mod-z"], UNDO).also(&["cmd-z", "ctrl-z"]),
            cmd!(Pii, Some(pii::POPUP_UNDO), &["mod-z"], UNDO).also(&["cmd-z", "ctrl-z"]),
            cmd!(Find, FIND, &["mod-z"], UNDO),
            cmd!(Find, REPLACE, &["mod-z"], UNDO),
            cmd!(App, Some(CHROME), &["mod-z"], UNDO),
        ],
    ),
    row(
        Everywhere,
        table::REDO.sheet,
        &[
            cmd!(Editor, EDITOR, &[], REDO)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"])
                .also(&["cmd-shift-z", "ctrl-shift-z", "ctrl-y"]),
            cmd!(Pii, Some("IdentityPanel"), &[], REDO)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"])
                .also(&["cmd-shift-z", "ctrl-shift-z", "ctrl-y"]),
            cmd!(Pii, Some(pii::POPUP_UNDO), &[], REDO)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"])
                .also(&["cmd-shift-z", "ctrl-shift-z", "ctrl-y"]),
            cmd!(Find, FIND, &[], REDO)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"]),
            cmd!(Find, REPLACE, &[], REDO)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"]),
            cmd!(App, Some(CHROME), &[], REDO)
                .mac(&["cmd-shift-z"])
                .other(&["ctrl-shift-z", "ctrl-y"]),
        ],
    ),
    // Layouts where plus needs Shift (US: Shift + =) report the Shift.
    row(
        Everywhere,
        table::ZOOM_IN.sheet,
        &[cmd!(App, None, &["mod-="], ZOOM_IN).also(&["mod-+", "mod-shift-=", "mod-shift-+"])],
    ),
    row(
        Everywhere,
        table::ZOOM_OUT.sheet,
        &[cmd!(App, None, &["mod--"], ZOOM_OUT)],
    ),
    row(
        Everywhere,
        table::ACTUAL_SIZE.sheet,
        &[cmd!(App, None, &["mod-0"], ACTUAL_SIZE)],
    ),
    row(
        Everywhere,
        table::SIDE_BY_SIDE.sheet,
        &[cmd!(App, None, &["mod-\\"], SIDE_BY_SIDE)],
    ),
    row(
        Everywhere,
        table::PSEUDONYMIZE.sheet,
        &[cmd!(Pii, None, &["mod-shift-p"], PSEUDONYMIZE)],
    ),
    row(
        Everywhere,
        text::APPLY_ALL_REPLACEMENTS,
        &[on!(Pii, None, &["mod-enter"], pii::PiiApplyAll)],
    ),
    row(
        Everywhere,
        table::COPY_FOR_AI.sheet,
        &[cmd!(App, None, &["mod-shift-c"], COPY_FOR_AI)],
    ),
    // ⌥⇧⌘C (the grill's pick) is Pseudonymize clipboard's; M for Markdown.
    row(
        Everywhere,
        table::COPY_MARKDOWN.sheet,
        &[cmd!(App, None, &["mod-shift-m"], COPY_MARKDOWN)],
    ),
    row(
        Everywhere,
        table::PASTE_AI_REPLY.sheet,
        &[cmd!(App, None, &["mod-alt-shift-v"], PASTE_AI_REPLY)],
    ),
    row(
        Everywhere,
        table::PSEUDONYMIZE_CLIPBOARD.sheet,
        &[cmd!(
            App,
            None,
            &["mod-alt-shift-c"],
            PSEUDONYMIZE_CLIPBOARD
        )],
    ),
    row(
        Everywhere,
        table::HIDE_MDOC.sheet,
        &[cmd!(Menus, None, &[], HIDE_MDOC).mac(&["cmd-h"])],
    ),
    row(
        Everywhere,
        table::HIDE_OTHERS.sheet,
        &[cmd!(Menus, None, &[], HIDE_OTHERS).mac(&["cmd-alt-h"])],
    ),
    row(
        Everywhere,
        table::MINIMIZE.sheet,
        &[cmd!(Menus, None, &[], MINIMIZE).mac(&["cmd-m"])],
    ),
    row(
        Everywhere,
        table::QUIT.sheet,
        &[cmd!(App, None, &["mod-q"], QUIT)],
    ),
    // ---- Editor -------------------------------------------------------------
    row(
        Editor,
        text::NEW_PARAGRAPH,
        &[on!(Editor, EDITOR, &["enter"], mdoc_editor::Newline)],
    ),
    row(
        Editor,
        text::LINE_BREAK,
        &[on!(
            Editor,
            EDITOR,
            &["shift-enter"],
            mdoc_editor::LineBreak
        )],
    ),
    row(
        Editor,
        text::INDENT,
        &[on!(Editor, EDITOR, &["tab"], mdoc_editor::Indent)],
    ),
    row(
        Editor,
        text::OUTDENT,
        &[on!(Editor, EDITOR, &["shift-tab"], mdoc_editor::Outdent)],
    ),
    row(
        Editor,
        text::BOLD,
        &[on!(Editor, EDITOR, &["mod-b"], mdoc_editor::Bold).also(&["cmd-b", "ctrl-b"])],
    ),
    row(
        Editor,
        text::ITALIC,
        &[on!(Editor, EDITOR, &["mod-i"], mdoc_editor::Italic).also(&["cmd-i", "ctrl-i"])],
    ),
    row(
        Editor,
        text::UNDERLINE,
        &[on!(Editor, EDITOR, &["mod-u"], mdoc_editor::Underline).also(&["cmd-u", "ctrl-u"])],
    ),
    row(
        Editor,
        text::STRIKETHROUGH,
        &[on!(Editor, EDITOR, &["mod-shift-x"], mdoc_editor::Strike)
            .also(&["cmd-shift-x", "ctrl-shift-x"])],
    ),
    row(
        Editor,
        text::CODE,
        &[on!(Editor, EDITOR, &["mod-e"], mdoc_editor::Code).also(&["cmd-e", "ctrl-e"])],
    ),
    row(
        Editor,
        table::SELECT_ALL.sheet,
        &[
            cmd!(Editor, EDITOR, &["mod-a"], SELECT_ALL).also(&["cmd-a", "ctrl-a"]),
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
        table::COPY.sheet,
        &[
            cmd!(Editor, EDITOR, &["mod-c"], COPY).also(&["cmd-c", "ctrl-c"]),
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
        table::CUT.sheet,
        &[
            cmd!(Editor, EDITOR, &["mod-x"], CUT).also(&["cmd-x", "ctrl-x"]),
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
        table::PASTE.sheet,
        &[
            cmd!(Editor, EDITOR, &["mod-v"], PASTE).also(&["cmd-v", "ctrl-v"]),
            on!(
                TextField,
                Some(markdown_search::TEXT_FIELDS),
                &["mod-v"],
                markdown_search::Paste
            )
            .also(&["cmd-v", "ctrl-v"]),
        ],
    ),
    // Caret moves: macOS keeps ⌥ for words and ⌘ for line and
    // document ends; Windows and Linux use Ctrl and Home / End.
    row(
        Editor,
        text::PREVIOUS_WORD,
        &[on!(Editor, EDITOR, &[], mdoc_editor::WordLeft)
            .mac(&["alt-left"])
            .other(&["ctrl-left"])
            .also(&["alt-left"])],
    ),
    row(
        Editor,
        text::NEXT_WORD,
        &[
            on!(Editor, EDITOR, &[], mdoc_editor::WordRight)
                .mac(&["alt-right"])
                .also(&["alt-right"]),
            on!(Editor, EDITOR, &[], mdoc_editor::NextWordStart).other(&["ctrl-right"]),
        ],
    ),
    row(
        Editor,
        text::SELECT_TO_PREVIOUS_WORD,
        &[on!(Editor, EDITOR, &[], mdoc_editor::SelectWordLeft)
            .mac(&["alt-shift-left"])
            .other(&["ctrl-shift-left"])
            .also(&["alt-shift-left"])],
    ),
    row(
        Editor,
        text::SELECT_TO_NEXT_WORD,
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
        text::LINE_START,
        &[on!(Editor, EDITOR, &[], mdoc_editor::LineStart).mac(&["cmd-left"])],
    ),
    row(
        Editor,
        text::LINE_END,
        &[on!(Editor, EDITOR, &[], mdoc_editor::LineEnd).mac(&["cmd-right"])],
    ),
    row(
        Editor,
        text::SELECT_TO_LINE_START,
        &[on!(Editor, EDITOR, &[], mdoc_editor::SelectToLineStart).mac(&["cmd-shift-left"])],
    ),
    row(
        Editor,
        text::SELECT_TO_LINE_END,
        &[on!(Editor, EDITOR, &[], mdoc_editor::SelectToLineEnd).mac(&["cmd-shift-right"])],
    ),
    row(
        Editor,
        text::DOCUMENT_START,
        &[on!(Editor, EDITOR, &[], mdoc_editor::DocumentStart)
            .mac(&["cmd-up"])
            .other(&["ctrl-home"])],
    ),
    row(
        Editor,
        text::DOCUMENT_END,
        &[on!(Editor, EDITOR, &[], mdoc_editor::DocumentEnd)
            .mac(&["cmd-down"])
            .other(&["ctrl-end"])],
    ),
    row(
        Editor,
        text::SELECT_TO_DOCUMENT_START,
        &[on!(Editor, EDITOR, &[], mdoc_editor::SelectToDocumentStart)
            .mac(&["cmd-shift-up"])
            .other(&["ctrl-shift-home"])],
    ),
    row(
        Editor,
        text::SELECT_TO_DOCUMENT_END,
        &[on!(Editor, EDITOR, &[], mdoc_editor::SelectToDocumentEnd)
            .mac(&["cmd-shift-down"])
            .other(&["ctrl-shift-end"])],
    ),
    row(
        Editor,
        text::DELETE_PREVIOUS_WORD,
        &[on!(Editor, EDITOR, &[], mdoc_editor::DeleteWordLeft)
            .mac(&["alt-backspace"])
            .other(&["ctrl-backspace"])],
    ),
    row(
        Editor,
        text::DELETE_NEXT_WORD,
        &[
            on!(Editor, EDITOR, &[], mdoc_editor::DeleteToWordEnd).mac(&["alt-delete"]),
            on!(Editor, EDITOR, &[], mdoc_editor::DeleteWordRight).other(&["ctrl-delete"]),
        ],
    ),
    row(
        Editor,
        text::DELETE_TO_LINE_START,
        &[on!(Editor, EDITOR, &[], mdoc_editor::DeleteToLineStart).mac(&["cmd-backspace"])],
    ),
    // Find and Replace keys follow Zed's defaults.
    row(
        Editor,
        table::FIND.sheet,
        &[
            cmd!(Find, EDITOR, &["mod-f"], FIND),
            cmd!(Find, None, &["mod-f"], FIND),
            cmd!(Find, FIND, &["mod-f"], FIND),
            cmd!(Find, REPLACE, &["mod-f"], FIND),
        ],
    ),
    row(
        Editor,
        table::FIND_AND_REPLACE.sheet,
        &[
            cmd!(Find, EDITOR, &[], FIND_AND_REPLACE)
                .mac(&["cmd-alt-f"])
                .other(&["ctrl-h"]),
            cmd!(Find, None, &[], FIND_AND_REPLACE)
                .mac(&["cmd-alt-f"])
                .other(&["ctrl-h"]),
            cmd!(Find, FIND, &[], FIND_AND_REPLACE)
                .mac(&["cmd-alt-f"])
                .other(&["ctrl-h"]),
            cmd!(Find, REPLACE, &[], FIND_AND_REPLACE)
                .mac(&["cmd-alt-f"])
                .other(&["ctrl-h"]),
        ],
    ),
    row(
        Editor,
        table::FIND_NEXT.sheet,
        &[
            cmd!(Find, EDITOR, &["mod-g"], FIND_NEXT),
            cmd!(Find, FIND, &["mod-g"], FIND_NEXT),
            cmd!(Find, REPLACE, &["mod-g"], FIND_NEXT),
        ],
    ),
    row(
        Editor,
        table::FIND_PREVIOUS.sheet,
        &[
            cmd!(Find, EDITOR, &["mod-shift-g"], FIND_PREVIOUS),
            cmd!(Find, FIND, &["mod-shift-g"], FIND_PREVIOUS),
            cmd!(Find, REPLACE, &["mod-shift-g"], FIND_PREVIOUS),
        ],
    ),
    row(
        Editor,
        text::NEXT_MATCH_IN_THE_FIND_FIELD,
        &[on!(Find, FIND, &["enter"], find::FindNextMarkdown)],
    ),
    row(
        Editor,
        text::PREVIOUS_MATCH_IN_THE_FIND_FIELD,
        &[on!(
            Find,
            FIND,
            &["shift-enter"],
            find::FindPreviousMarkdown
        )],
    ),
    row(
        Editor,
        text::REPLACE_AND_FIND_NEXT_IN_THE_REPLACE_FIELD,
        &[on!(Find, REPLACE, &["enter"], find::ReplaceNextMarkdown)],
    ),
    row(
        Editor,
        text::REPLACE_ALL,
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
        text::MATCH_CASE,
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
        text::WHOLE_WORD,
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
        text::CLOSE_FIND,
        &[
            on!(Find, FIND, &["escape"], find::CloseMarkdownSearch),
            on!(Find, REPLACE, &["escape"], find::CloseMarkdownSearch),
        ],
    ),
    row(
        Editor,
        text::NEXT_MISSPELLING,
        &[on!(App, None, &["mod-;"], spelling::NextMisspelling)],
    ),
    row(
        Editor,
        text::PREVIOUS_MISSPELLING,
        &[on!(
            App,
            None,
            &["mod-shift-;"],
            spelling::PreviousMisspelling
        )],
    ),
    row(
        Editor,
        text::REVIEW_THE_PROPOSAL_AT_THE_CARET,
        &[on!(Pii, EDITOR, &["alt-enter"], pii::PiiReviewCandidate)],
    ),
    row(
        Editor,
        text::NEXT_PROPOSAL,
        &[on!(Pii, EDITOR, &["alt-down"], pii::PiiNextCandidate)],
    ),
    row(
        Editor,
        text::PREVIOUS_PROPOSAL,
        &[on!(Pii, EDITOR, &["alt-up"], pii::PiiPreviousCandidate)],
    ),
    row(
        Editor,
        text::PROPOSE_THE_SELECTION,
        &[on!(Pii, EDITOR, &["mod-alt-p"], pii::PiiAddCandidate)],
    ),
    row(
        Editor,
        text::PROPOSE_THE_SELECTION_AS,
        &[on!(
            Pii,
            EDITOR,
            &["mod-alt-shift-p"],
            pii::PiiAddCandidateAs
        )],
    ),
    // Esc leaves side by side, also from inside its read-only columns.
    row(
        Editor,
        text::LEAVE_SIDE_BY_SIDE,
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
    // The sidebar's tree is one Tab stop; arrows move inside it.
    row(
        Panel,
        text::SIDEBAR_PREVIOUS_ROW,
        &[on!(
            Ui,
            Some("SidebarRow"),
            &["up"],
            crate::ui::SidebarPrevious
        )],
    ),
    row(
        Panel,
        text::SIDEBAR_NEXT_ROW,
        &[on!(
            Ui,
            Some("SidebarRow"),
            &["down"],
            crate::ui::SidebarNext
        )],
    ),
    row(
        Panel,
        text::SIDEBAR_COLLAPSE_A_PROJECT_OR_GO_TO_IT,
        &[on!(
            Ui,
            Some("SidebarRow"),
            &["left"],
            crate::ui::SidebarCollapse
        )],
    ),
    row(
        Panel,
        text::SIDEBAR_EXPAND_A_PROJECT_OR_GO_INTO_IT,
        &[on!(
            Ui,
            Some("SidebarRow"),
            &["right"],
            crate::ui::SidebarExpand
        )],
    ),
    row(
        Panel,
        text::UNFILED_SELECT_THE_ROW_ABOVE_TOO,
        &[on!(
            Ui,
            Some("SidebarRow"),
            &["shift-up"],
            crate::ui::SidebarExtendUp
        )],
    ),
    row(
        Panel,
        text::UNFILED_SELECT_THE_ROW_BELOW_TOO,
        &[on!(
            Ui,
            Some("SidebarRow"),
            &["shift-down"],
            crate::ui::SidebarExtendDown
        )],
    ),
    row(
        Panel,
        text::UNFILED_DELETE_THE_SELECTED_DOCUMENTS,
        &[on!(
            Ui,
            Some("SidebarRow"),
            &["backspace"],
            crate::ui::SidebarDelete
        )
        .also(&["delete"])],
    ),
    row(
        Panel,
        text::SIDEBAR_THE_ROW_S_MENU,
        // ⌃↩ needs no fn on MacBook keyboards, where F10 is the mute key;
        // Windows adds the Menu key.
        &[on!(
            Ui,
            Some("SidebarRow"),
            &["ctrl-enter", "shift-f10"],
            crate::ui::SidebarMenu
        )
        .other(&["ctrl-enter", "shift-f10", "menu"])],
    ),
    // The Replacements checklist.
    row(
        Panel,
        text::REPLACEMENTS_PREVIOUS_ROW,
        &[on!(Pii, Some(pii::LIST), &["up"], pii::PanelUp)],
    ),
    row(
        Panel,
        text::REPLACEMENTS_NEXT_ROW,
        &[on!(Pii, Some(pii::LIST), &["down"], pii::PanelDown)],
    ),
    row(
        Panel,
        text::REPLACEMENTS_FIRST_ROW,
        &[on!(Pii, Some(pii::LIST), &["mod-up"], pii::PanelFirst)],
    ),
    row(
        Panel,
        text::REPLACEMENTS_LAST_ROW,
        &[on!(Pii, Some(pii::LIST), &["mod-down"], pii::PanelLast)],
    ),
    row(
        Panel,
        text::REPLACEMENTS_MOVE_TO_THE_GROUP_ABOVE,
        &[on!(Pii, Some(pii::LIST), &["alt-up"], pii::PanelMoveUp)],
    ),
    row(
        Panel,
        text::REPLACEMENTS_MOVE_TO_THE_GROUP_BELOW,
        &[on!(Pii, Some(pii::LIST), &["alt-down"], pii::PanelMoveDown)],
    ),
    row(
        Panel,
        text::REPLACEMENTS_BACK_TO_THE_FILTER_THEN_CLOSE,
        &[on!(Pii, Some("IdentityPanel"), &["escape"], pii::PanelEscape).also(&["alt-escape"])],
    ),
    row(
        Panel,
        text::REPLACEMENTS_TICK_OR_UNTICK,
        &[on!(Pii, Some(pii::LIST), &["space"], pii::PanelToggle)],
    ),
    row(
        Panel,
        text::REPLACEMENTS_GO_TO_THE_MENTION,
        &[on!(Pii, Some(pii::LIST), &["enter"], pii::PanelOpen)],
    ),
    row(
        Panel,
        text::REPLACEMENTS_SHOW_MENTIONS,
        &[on!(Pii, Some(pii::LIST), &["right"], pii::PanelExpand)],
    ),
    row(
        Panel,
        text::REPLACEMENTS_HIDE_MENTIONS,
        &[on!(Pii, Some(pii::LIST), &["left"], pii::PanelCollapse)],
    ),
    row(
        Panel,
        text::REPLACEMENTS_KEEP_THE_ORIGINAL,
        &[on!(Pii, Some(pii::LIST), &["backspace"], pii::PanelKeep).also(&["delete"])],
    ),
    row(
        Panel,
        text::REPLACEMENTS_FROM_THE_FILTER_TO_THE_LIST,
        &[on!(
            Pii,
            Some("ReplacementSearch"),
            &["down"],
            pii::PanelEnterList
        )],
    ),
    // The source preview of a PDF or DOCX.
    row(
        Panel,
        text::SOURCE_NEXT_PAGE,
        &[on!(Pdf, Some("PdfView"), &["pagedown"], gpui_pdf::NextPage)],
    ),
    row(
        Panel,
        text::SOURCE_PREVIOUS_PAGE,
        &[on!(
            Pdf,
            Some("PdfView"),
            &["pageup"],
            gpui_pdf::PreviousPage
        )],
    ),
    row(
        Panel,
        text::SOURCE_FIRST_PAGE,
        &[on!(Pdf, Some("PdfView"), &["home"], gpui_pdf::FirstPage)],
    ),
    row(
        Panel,
        text::SOURCE_LAST_PAGE,
        &[on!(Pdf, Some("PdfView"), &["end"], gpui_pdf::LastPage)],
    ),
    row(
        Panel,
        text::SOURCE_NEXT_COMMENT,
        &[on!(
            App,
            Some("PdfView"),
            &["alt-down"],
            crate::document::comment_rail::NextComment
        )],
    ),
    row(
        Panel,
        text::SOURCE_PREVIOUS_COMMENT,
        &[on!(
            App,
            Some("PdfView"),
            &["alt-up"],
            crate::document::comment_rail::PreviousComment
        )],
    ),
    row(
        Panel,
        text::SOURCE_GO_TO_PAGE,
        &[on!(Pdf, Some("PdfView"), &[], gpui_pdf::GoToPage)
            .mac(&["cmd-alt-g"])
            .other(&["ctrl-alt-g"])],
    ),
    // ---- Popups, menus and dialogs ------------------------------------------
    // The mention popup.
    row(
        Popup,
        text::APPLY_AND_GO_TO_THE_NEXT_PROPOSAL,
        &[
            on!(Pii, Some(pii::POPUP), &["enter"], pii::ApplyThis),
            // Scope segments keep Space for selection, but Enter applies in
            // the chosen scope instead of pressing the segment again (#177).
            on!(Pii, Some("PiiScope"), &["enter"], pii::ApplyThis),
        ],
    ),
    row(
        Popup,
        text::TICK_OR_UNTICK_REPLACE,
        &[on!(Pii, Some(pii::POPUP), &["space"], pii::PanelKeep)],
    ),
    row(
        Popup,
        text::KEEP_THE_ORIGINAL_HERE,
        &[on!(Pii, Some(pii::POPUP), &["backspace"], pii::KeepThis).also(&["delete"])],
    ),
    row(
        Popup,
        text::KEEP_THIS_WORDING_EVERYWHERE,
        &[on!(Pii, Some(pii::POPUP), &["mod-backspace"], pii::KeepSame).also(&["mod-delete"])],
    ),
    row(
        Popup,
        text::KEEP_THE_WHOLE_ENTITY,
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
        text::PREVIOUS_MENTION,
        &[on!(Pii, Some(pii::STEP), &["left"], pii::PopupPreviousMention).also(&["up"])],
    ),
    row(
        Popup,
        text::NEXT_MENTION,
        &[on!(Pii, Some(pii::STEP), &["right"], pii::PopupNextMention).also(&["down"])],
    ),
    row(
        Popup,
        text::FIRST_MENTION,
        &[on!(Pii, Some(pii::STEP), &["mod-up"], pii::PopupFirstMention).also(&["mod-left"])],
    ),
    row(
        Popup,
        text::LAST_MENTION,
        &[on!(Pii, Some(pii::STEP), &["mod-down"], pii::PopupLastMention).also(&["mod-right"])],
    ),
    // In the alias field Enter confirms the draft.
    row(
        Popup,
        text::CONFIRM_THE_ALIAS,
        &[on!(
            Pii,
            Some("PseudonymReplacement"),
            &["enter"],
            pii::PiiConfirm
        )],
    ),
    row(
        Popup,
        text::CLOSE_THE_POPUP,
        &[on!(
            Pii,
            Some("PseudonymReview"),
            &["escape"],
            pii::PiiClosePopup
        )],
    ),
    row(
        Popup,
        text::CLOSE_A_MENU,
        &[
            on!(Ui, Some("UiMenu"), &["escape"], crate::ui::CloseMenu),
            on!(Pdf, Some("PdfMenu"), &["escape"], gpui_pdf::MenuClose),
        ],
    ),
    row(
        Panel,
        text::NOTES_CLOSE_THE_INSPECTOR,
        &[on!(
            Ui,
            Some("NotesBox > Editor"),
            &["escape"],
            crate::ui::CloseMenu
        )],
    ),
    row(
        Panel,
        text::OPEN_DOCUMENTS_WITH_THE_RECOVERY_KEY,
        &[on!(
            Shell,
            Some("RecoveryKey"),
            &["enter"],
            workspace::OpenWithRecoveryKey
        )],
    ),
    row(
        Popup,
        text::CLOSE_THE_COMMAND_PALETTE,
        &[on!(
            Shell,
            Some(workspace::PALETTE),
            &["escape"],
            workspace::PaletteClose
        )],
    ),
    row(
        Popup,
        text::CLOSE_KEYBOARD_SHORTCUTS,
        &[on!(
            Shell,
            Some(shortcuts_ui::SHEET),
            &["escape"],
            shortcuts_ui::CloseShortcuts
        )],
    ),
    row(
        Popup,
        text::DONT_SAVE_IN_A_SAVE_PROMPT,
        &[on!(
            Dialog,
            Some(confirm_dialog::DIALOG),
            &[],
            confirm_dialog::Discard
        )
        .mac(&["cmd-d"])
        .other(&["alt-d"])],
    ),
    row(
        Popup,
        text::CLOSE_SETTINGS,
        &[on!(
            Settings,
            Some("ModelSettings"),
            &["escape"],
            crate::settings::CloseSettings
        )],
    ),
    row(
        Panel,
        text::ADD_THE_WORDING_TO_THE_LIST,
        &[on!(
            Settings,
            Some("UserListInput"),
            &["enter"],
            crate::settings::lists::AddUserListEntry
        )],
    ),
    row(
        Popup,
        text::ADD_THE_WORD_TO_THE_DICTIONARY,
        &[on!(
            Dictionary,
            Some("DictionarySearch"),
            &["enter"],
            crate::settings::dictionary::AddDictionaryWord
        )],
    ),
    row(
        Popup,
        text::SAVE_THE_PROJECT_NAME,
        &[on!(
            Projects,
            Some("ProjectName"),
            &["enter"],
            workspace::ConfirmProjectName
        )],
    ),
    row(
        Popup,
        text::CANCEL_RENAMING_THE_PROJECT,
        &[on!(
            Projects,
            Some("ProjectName"),
            &["escape"],
            workspace::CancelProjectName
        )],
    ),
    row(
        Panel,
        text::ADD_A_NAME_TO_THE_PROJECT,
        &[on!(
            Projects,
            Some("ProjectTerm"),
            &["enter"],
            workspace::AddProjectName
        )],
    ),
    row(
        Panel,
        text::CLEAR_THE_NEW_PROJECT_NAME,
        &[on!(
            Projects,
            Some("ProjectTerm"),
            &["escape"],
            workspace::ClearProjectName
        )],
    ),
    row(
        Popup,
        text::CLOSE_THE_MODEL_COMPARISON,
        &[on!(
            Comparison,
            Some("ModelComparison"),
            &["escape"],
            comparison_ui::CloseComparison
        )],
    ),
    // The menu bar shelf: macOS only, ⌘C as a drag's counterpart.
    row(
        Popup,
        text::COPY_A_SHELF_DOCUMENT_FOR_AI,
        &[on!(Shelf, Some("Shelf"), &[], shelf_ui::ShelfCopyForAi).mac(&["cmd-c"])],
    ),
    row(
        Popup,
        text::COPY_SHELF_DOCUMENT_1_8_FOR_AI,
        &[
            on!(Shelf, Some("Shelf"), &[], shelf_ui::ShelfCopyRow(1)).mac(&["cmd-1"]),
            on!(Shelf, Some("Shelf"), &[], shelf_ui::ShelfCopyRow(2)).mac(&["cmd-2"]),
            on!(Shelf, Some("Shelf"), &[], shelf_ui::ShelfCopyRow(3)).mac(&["cmd-3"]),
            on!(Shelf, Some("Shelf"), &[], shelf_ui::ShelfCopyRow(4)).mac(&["cmd-4"]),
            on!(Shelf, Some("Shelf"), &[], shelf_ui::ShelfCopyRow(5)).mac(&["cmd-5"]),
            on!(Shelf, Some("Shelf"), &[], shelf_ui::ShelfCopyRow(6)).mac(&["cmd-6"]),
            on!(Shelf, Some("Shelf"), &[], shelf_ui::ShelfCopyRow(7)).mac(&["cmd-7"]),
            on!(Shelf, Some("Shelf"), &[], shelf_ui::ShelfCopyRow(8)).mac(&["cmd-8"]),
        ],
    ),
];

#[cfg(test)]
mod tests;
