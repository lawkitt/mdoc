//! The words of what the user can ask of mdoc: each command's name in the
//! palette, the ⋯ menu, the shortcuts sheet and the menu bar, the menus'
//! titles, the sheet's groups and its other rows, and the keys' names. Each
//! is a constant here, typed once.

// ---- The menu bar's menus -------------------------------------------------------

/// The application menu, which macOS titles with the app's name.
pub(crate) const MENU_APP: &str = "mdoc";
pub(crate) const MENU_FILE: &str = "File";
pub(crate) const MENU_PROJECT: &str = "Project";
pub(crate) const MENU_EDIT: &str = "Edit";
pub(crate) const MENU_VIEW: &str = "View";
pub(crate) const MENU_WINDOW: &str = "Window";
pub(crate) const MENU_HELP: &str = "Help";
/// The application menu's submenu that macOS fills.
pub(crate) const SERVICES: &str = "Services";

// ---- The commands -----------------------------------------------------------------
// A command's name in the palette, the ⋯ menu and the sheet, then, where the
// menu bar words it otherwise, its name there.

pub(crate) const ABOUT_MDOC: &str = "About mdoc";
pub(crate) const CHECK_FOR_UPDATES: &str = "Check for Updates…";
pub(crate) const SETTINGS: &str = "Settings";
pub(crate) const SETTINGS_MENU: &str = "Settings…";
pub(crate) const HIDE_MDOC: &str = "Hide mdoc";
pub(crate) const HIDE_OTHERS: &str = "Hide others";
pub(crate) const HIDE_OTHERS_MENU: &str = "Hide Others";
pub(crate) const SHOW_ALL: &str = "Show All";
pub(crate) const QUIT: &str = "Quit";
pub(crate) const QUIT_MENU: &str = "Quit mdoc";

pub(crate) const NEW_DOCUMENT: &str = "New document";
pub(crate) const NEW_MENU: &str = "New";
/// New in the sheet.
pub(crate) const NEW_TAB: &str = "New tab";
pub(crate) const OPEN_FILES: &str = "Open files…";
pub(crate) const OPEN_MENU: &str = "Open…";
pub(crate) const OPEN_A_PROJECT: &str = "Open a Project…";
pub(crate) const OPEN_PROJECT_MENU: &str = "Open Project…";
pub(crate) const CONVERT_TO_MARKDOWN: &str = "Convert to Markdown";
pub(crate) const SAVE: &str = "Save";
pub(crate) const EXPORT_MARKDOWN: &str = "Export Markdown…";
pub(crate) const COPY_MARKDOWN: &str = "Copy Markdown";
pub(crate) const COPY_FOR_AI: &str = "Copy for AI";
pub(crate) const PASTE_AI_REPLY: &str = "Paste AI reply";
pub(crate) const PASTE_AI_REPLY_MENU: &str = "Paste AI Reply";

pub(crate) const NEW_PROJECT: &str = "New Project…";
pub(crate) const MOVE_TO_PROJECT: &str = "Move to Project…";
pub(crate) const MOVE_TO_NEW_PROJECT: &str = "Move to New Project";
pub(crate) const ADD_DOCUMENT_TO_PROJECT: &str = "Add document to Project…";
pub(crate) const ADD_DOCUMENT_MENU: &str = "Add Document…";
pub(crate) const RENAME_PROJECT: &str = "Rename Project";
pub(crate) const PROJECT_COLOUR: &str = "Project colour";
pub(crate) const PROJECT_COLOUR_MENU: &str = "Project Colour";
pub(crate) const PROJECT_CATEGORIES: &str = "Project categories to detect";
pub(crate) const PROJECT_CATEGORIES_MENU: &str = "Categories to Detect";
pub(crate) const EXPORT_PROJECT: &str = "Export Project…";
pub(crate) const DELETE_PROJECT: &str = "Delete Project…";

pub(crate) const UNDO: &str = "Undo";
pub(crate) const REDO: &str = "Redo";
pub(crate) const CUT: &str = "Cut";
pub(crate) const COPY: &str = "Copy";
pub(crate) const PASTE: &str = "Paste";
pub(crate) const SELECT_ALL: &str = "Select all";
pub(crate) const SELECT_ALL_MENU: &str = "Select All";
pub(crate) const FIND: &str = "Find";
pub(crate) const FIND_MENU: &str = "Find…";
pub(crate) const FIND_AND_REPLACE: &str = "Find and replace";
pub(crate) const FIND_AND_REPLACE_MENU: &str = "Find and Replace…";
pub(crate) const FIND_NEXT: &str = "Find next";
pub(crate) const FIND_NEXT_MENU: &str = "Find Next";
pub(crate) const FIND_PREVIOUS: &str = "Find previous";
pub(crate) const FIND_PREVIOUS_MENU: &str = "Find Previous";
pub(crate) const PSEUDONYMIZE: &str = "Pseudonymize";
pub(crate) const PSEUDONYMIZE_CLIPBOARD: &str = "Pseudonymize clipboard";
pub(crate) const PSEUDONYMIZE_CLIPBOARD_MENU: &str = "Pseudonymize Clipboard";
pub(crate) const RESTORE_CLIPBOARD_MENU: &str = "Restore Clipboard";

pub(crate) const HOME: &str = "Home";
pub(crate) const COMMAND_PALETTE: &str = "Command palette";
/// The palette in the menu bar.
pub(crate) const SEARCH_MENU: &str = "Search";
pub(crate) const TOGGLE_SIDEBAR: &str = "Toggle sidebar";
pub(crate) const TOGGLE_SIDEBAR_MENU: &str = "Toggle Sidebar";
pub(crate) const EDIT: &str = "Edit";
pub(crate) const ORIGINAL: &str = "Original";
pub(crate) const PSEUDONYMIZED: &str = "Pseudonymized";
pub(crate) const SIDE_BY_SIDE: &str = "Side by side";
pub(crate) const SIDE_BY_SIDE_MENU: &str = "Side by Side";
pub(crate) const SHOW_REPLACEMENTS: &str = "Show Replacements";
pub(crate) const NOTES: &str = "Notes";
pub(crate) const OPEN_SOURCE_REPLY: &str = "Open source AI reply";
pub(crate) const OPEN_SOURCE_REPLY_MENU: &str = "Open Source AI Reply";
pub(crate) const ZOOM_IN: &str = "Zoom in";
pub(crate) const ZOOM_IN_MENU: &str = "Zoom In";
pub(crate) const ZOOM_OUT: &str = "Zoom out";
pub(crate) const ZOOM_OUT_MENU: &str = "Zoom Out";
pub(crate) const ACTUAL_SIZE: &str = "Actual size";
pub(crate) const ACTUAL_SIZE_MENU: &str = "Actual Size";

pub(crate) const MINIMIZE: &str = "Minimize";
/// The window's zoom, in the Window menu.
pub(crate) const ZOOM: &str = "Zoom";
pub(crate) const SWITCH_TO_A_RECENT_DOCUMENT: &str = "Switch to a recent document";
pub(crate) const SWITCH_DOCUMENT_MENU: &str = "Switch Document";
pub(crate) const SWITCH_BACKWARDS: &str = "Switch to a recent document, backwards";
pub(crate) const SWITCH_DOCUMENT_BACK_MENU: &str = "Switch Document Back";
pub(crate) const MDOC_HELP: &str = "mdoc Help";
pub(crate) const KEYBOARD_SHORTCUTS: &str = "Keyboard shortcuts";
pub(crate) const KEYBOARD_SHORTCUTS_MENU: &str = "Keyboard Shortcuts";

// ---- The shortcuts sheet ----------------------------------------------------------

/// Where a shortcut works, as the sheet groups them.
pub(crate) const EVERYWHERE: &str = "Everywhere";
pub(crate) const EDITOR: &str = "Editor";
pub(crate) const POPUPS: &str = "Popups, menus and dialogs";
pub(crate) const PANELS: &str = "Panels and sidebar";

/// The sheet's rows that name no command of the menus, in the table's order.
pub(crate) const BACK: &str = "Back";
pub(crate) const FORWARD: &str = "Forward";
pub(crate) const NEXT_REGION: &str = "Next region";
pub(crate) const PREVIOUS_REGION: &str = "Previous region";
pub(crate) const NEXT_CONTROL: &str = "Next control";
pub(crate) const PREVIOUS_CONTROL: &str = "Previous control";
pub(crate) const BACK_TO_THE_EDITOR_FROM_THE_SIDEBAR_OR_TOOLBAR: &str =
    "Back to the editor from the sidebar or toolbar";
pub(crate) const CLOSE_NOTICES: &str = "Close notices";
pub(crate) const APPLY_ALL_REPLACEMENTS: &str = "Apply all replacements";
pub(crate) const NEW_PARAGRAPH: &str = "New paragraph";
pub(crate) const LINE_BREAK: &str = "Line break";
pub(crate) const INDENT: &str = "Indent";
pub(crate) const OUTDENT: &str = "Outdent";
pub(crate) const BOLD: &str = "Bold";
pub(crate) const ITALIC: &str = "Italic";
pub(crate) const UNDERLINE: &str = "Underline";
pub(crate) const STRIKETHROUGH: &str = "Strikethrough";
pub(crate) const CODE: &str = "Code";
pub(crate) const PREVIOUS_WORD: &str = "Previous word";
pub(crate) const NEXT_WORD: &str = "Next word";
pub(crate) const SELECT_TO_PREVIOUS_WORD: &str = "Select to previous word";
pub(crate) const SELECT_TO_NEXT_WORD: &str = "Select to next word";
pub(crate) const LINE_START: &str = "Line start";
pub(crate) const LINE_END: &str = "Line end";
pub(crate) const SELECT_TO_LINE_START: &str = "Select to line start";
pub(crate) const SELECT_TO_LINE_END: &str = "Select to line end";
pub(crate) const DOCUMENT_START: &str = "Document start";
pub(crate) const DOCUMENT_END: &str = "Document end";
pub(crate) const SELECT_TO_DOCUMENT_START: &str = "Select to document start";
pub(crate) const SELECT_TO_DOCUMENT_END: &str = "Select to document end";
pub(crate) const DELETE_PREVIOUS_WORD: &str = "Delete previous word";
pub(crate) const DELETE_NEXT_WORD: &str = "Delete next word";
pub(crate) const DELETE_TO_LINE_START: &str = "Delete to line start";
pub(crate) const NEXT_MATCH_IN_THE_FIND_FIELD: &str = "Next match, in the find field";
pub(crate) const PREVIOUS_MATCH_IN_THE_FIND_FIELD: &str = "Previous match, in the find field";
pub(crate) const REPLACE_AND_FIND_NEXT_IN_THE_REPLACE_FIELD: &str =
    "Replace and find next, in the replace field";
pub(crate) const REPLACE_ALL: &str = "Replace all";
pub(crate) const MATCH_CASE: &str = "Match case";
pub(crate) const WHOLE_WORD: &str = "Whole word";
pub(crate) const CLOSE_FIND: &str = "Close find";
pub(crate) const NEXT_MISSPELLING: &str = "Next misspelling";
pub(crate) const PREVIOUS_MISSPELLING: &str = "Previous misspelling";
pub(crate) const REVIEW_THE_PROPOSAL_AT_THE_CARET: &str = "Review the proposal at the caret";
pub(crate) const NEXT_PROPOSAL: &str = "Next proposal";
pub(crate) const PREVIOUS_PROPOSAL: &str = "Previous proposal";
pub(crate) const PROPOSE_THE_SELECTION: &str = "Propose the selection";
pub(crate) const PROPOSE_THE_SELECTION_AS: &str = "Propose the selection as…";
pub(crate) const LEAVE_SIDE_BY_SIDE: &str = "Leave side by side";
pub(crate) const SIDEBAR_PREVIOUS_ROW: &str = "Sidebar: previous row";
pub(crate) const SIDEBAR_NEXT_ROW: &str = "Sidebar: next row";
pub(crate) const SIDEBAR_COLLAPSE_A_PROJECT_OR_GO_TO_IT: &str =
    "Sidebar: collapse a Project, or go to it";
pub(crate) const SIDEBAR_EXPAND_A_PROJECT_OR_GO_INTO_IT: &str =
    "Sidebar: expand a Project, or go into it";
pub(crate) const UNFILED_SELECT_THE_ROW_ABOVE_TOO: &str = "Unfiled: select the row above too";
pub(crate) const UNFILED_SELECT_THE_ROW_BELOW_TOO: &str = "Unfiled: select the row below too";
pub(crate) const UNFILED_DELETE_THE_SELECTED_DOCUMENTS: &str =
    "Unfiled: delete the selected documents";
pub(crate) const SIDEBAR_THE_ROW_S_MENU: &str = "Sidebar: the row's menu";
pub(crate) const REPLACEMENTS_PREVIOUS_ROW: &str = "Replacements: previous row";
pub(crate) const REPLACEMENTS_NEXT_ROW: &str = "Replacements: next row";
pub(crate) const REPLACEMENTS_FIRST_ROW: &str = "Replacements: first row";
pub(crate) const REPLACEMENTS_LAST_ROW: &str = "Replacements: last row";
pub(crate) const REPLACEMENTS_MOVE_TO_THE_GROUP_ABOVE: &str =
    "Replacements: move to the group above";
pub(crate) const REPLACEMENTS_MOVE_TO_THE_GROUP_BELOW: &str =
    "Replacements: move to the group below";
pub(crate) const REPLACEMENTS_BACK_TO_THE_FILTER_THEN_CLOSE: &str =
    "Replacements: back to the filter, then close";
pub(crate) const REPLACEMENTS_TICK_OR_UNTICK: &str = "Replacements: tick or untick";
pub(crate) const REPLACEMENTS_GO_TO_THE_MENTION: &str = "Replacements: go to the mention";
pub(crate) const REPLACEMENTS_SHOW_MENTIONS: &str = "Replacements: show mentions";
pub(crate) const REPLACEMENTS_HIDE_MENTIONS: &str = "Replacements: hide mentions";
pub(crate) const REPLACEMENTS_KEEP_THE_ORIGINAL: &str = "Replacements: keep the original";
pub(crate) const REPLACEMENTS_FROM_THE_FILTER_TO_THE_LIST: &str =
    "Replacements: from the filter to the list";
pub(crate) const SOURCE_NEXT_PAGE: &str = "Source: next page";
pub(crate) const SOURCE_PREVIOUS_PAGE: &str = "Source: previous page";
pub(crate) const SOURCE_FIRST_PAGE: &str = "Source: first page";
pub(crate) const SOURCE_LAST_PAGE: &str = "Source: last page";
pub(crate) const SOURCE_NEXT_COMMENT: &str = "Source: next comment";
pub(crate) const SOURCE_PREVIOUS_COMMENT: &str = "Source: previous comment";
pub(crate) const SOURCE_GO_TO_PAGE: &str = "Source: go to page";
pub(crate) const APPLY_AND_GO_TO_THE_NEXT_PROPOSAL: &str = "Apply and go to the next proposal";
pub(crate) const TICK_OR_UNTICK_REPLACE: &str = "Tick or untick Replace";
pub(crate) const KEEP_THE_ORIGINAL_HERE: &str = "Keep the original here";
pub(crate) const KEEP_THIS_WORDING_EVERYWHERE: &str = "Keep this wording everywhere";
pub(crate) const KEEP_THE_WHOLE_ENTITY: &str = "Keep the whole entity";
pub(crate) const PREVIOUS_MENTION: &str = "Previous mention";
pub(crate) const NEXT_MENTION: &str = "Next mention";
pub(crate) const FIRST_MENTION: &str = "First mention";
pub(crate) const LAST_MENTION: &str = "Last mention";
pub(crate) const CONFIRM_THE_ALIAS: &str = "Confirm the alias";
pub(crate) const CLOSE_THE_POPUP: &str = "Close the popup";
pub(crate) const CLOSE_A_MENU: &str = "Close a menu";
pub(crate) const NOTES_CLOSE_THE_INSPECTOR: &str = "Notes: close the Inspector";
pub(crate) const OPEN_DOCUMENTS_WITH_THE_RECOVERY_KEY: &str =
    "Open documents with the recovery key";
pub(crate) const CLOSE_THE_COMMAND_PALETTE: &str = "Close the command palette";
pub(crate) const CLOSE_KEYBOARD_SHORTCUTS: &str = "Close keyboard shortcuts";
pub(crate) const DONT_SAVE_IN_A_SAVE_PROMPT: &str = "Don't Save, in a save prompt";
pub(crate) const CLOSE_SETTINGS: &str = "Close Settings";
pub(crate) const ADD_THE_WORDING_TO_THE_LIST: &str = "Add the wording to the list";
pub(crate) const ADD_THE_WORD_TO_THE_DICTIONARY: &str = "Add the word to the dictionary";
pub(crate) const SAVE_THE_PROJECT_NAME: &str = "Save the Project name";
pub(crate) const CANCEL_RENAMING_THE_PROJECT: &str = "Cancel renaming the Project";
pub(crate) const ADD_A_NAME_TO_THE_PROJECT: &str = "Add a name to the Project";
pub(crate) const CLEAR_THE_NEW_PROJECT_NAME: &str = "Clear the new Project name";
pub(crate) const CLOSE_THE_MODEL_COMPARISON: &str = "Close the model comparison";
pub(crate) const COPY_A_SHELF_DOCUMENT_FOR_AI: &str = "Copy a shelf document for AI";
pub(crate) const COPY_SHELF_DOCUMENT_1_8_FOR_AI: &str = "Copy shelf document 1–8 for AI";

// ---- The keys' names ------------------------------------------------------------
// As the sheet, the palette and tooltips write a key; macOS writes its
// modifiers and a few keys as symbols instead.

pub(crate) const KEY_ENTER: &str = "Enter";
pub(crate) const KEY_BACKSPACE: &str = "Backspace";
pub(crate) const KEY_DELETE: &str = "Delete";
pub(crate) const KEY_ESC: &str = "Esc";
pub(crate) const KEY_MENU: &str = "Menu";
pub(crate) const KEY_TAB: &str = "Tab";
pub(crate) const KEY_SPACE: &str = "Space";
pub(crate) const KEY_HOME: &str = "Home";
pub(crate) const KEY_PAGE_UP: &str = "Page Up";
pub(crate) const KEY_PAGE_DOWN: &str = "Page Down";
pub(crate) const KEY_END: &str = "End";
pub(crate) const KEY_CTRL: &str = "Ctrl";
pub(crate) const KEY_ALT: &str = "Alt";
pub(crate) const KEY_SHIFT: &str = "Shift";
pub(crate) const KEY_WIN: &str = "Win";
