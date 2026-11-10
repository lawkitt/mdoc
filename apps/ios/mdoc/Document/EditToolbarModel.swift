import MdocFFI
import Observation
import UIKit

/// A button of edit mode's toolbar or formatting keyboard, or a hardware
/// shortcut (ADR 0109): each is one editor-core call, applied as one edit
/// and one "Formatting" undo step.
enum EditCommand: Hashable {
    case format(EditorInlineFormat)
    case list
    case task
    case indent
    case outdent
    case turnInto(EditorBlockKind)
    case rule
    /// The formatting keyboard's Table: a new table.
    case insertTable
    /// A table control on the caret's table (ADR 0110): rows, columns,
    /// alignment, delete.
    case table(EditorTableCommand)
    /// Tab, ⇧Tab or an arrow key from a hardware keyboard in a cell: no
    /// edit, but Tab from the last cell, which leaves the table.
    case moveInTable(EditorTableMove)
    /// ⇧Return in a cell: `<br>`.
    case lineBreak

    /// The core's change for `selection` (UTF-8) in `text`; nil when the
    /// command does not apply there.
    func change(text: String, selection: TextRange, rendered: Bool) -> EditorChange? {
        switch self {
        case .format(let format): editorToggleFormat(text: text, selection: selection, format: format)
        case .list: editorToggleList(text: text, selection: selection)
        case .task: editorTaskButton(text: text, selection: selection)
        case .indent: editorIndent(text: text, selection: selection, rendered: rendered)
        case .outdent: editorOutdent(text: text, selection: selection, rendered: rendered)
        case .turnInto(let kind): editorTurnInto(text: text, selection: selection, kind: kind)
        case .rule: editorInsertRule(text: text, selection: selection)
        case .insertTable: editorInsertTable(text: text, selection: selection)
        case .table(let command): editorTableCommand(text: text, selection: selection, command: command)
        case .moveInTable(let move): editorTableMove(text: text, selection: selection, by: move, rendered: rendered)
        case .lineBreak: editorShiftEnter(text: text, selection: selection, rendered: rendered)
        }
    }
}

/// What edit mode's toolbar and formatting keyboard show and do (ADR 0109).
/// The text view's coordinator keeps it: the caret state comes from the
/// editor core (`editor_caret_state`) after each caret move or edit, and
/// the buttons call back into the coordinator, which applies the core's
/// change through the typing path.
@MainActor
@Observable
final class EditToolbarModel {
    /// The core's answer for the selection; nil before the first.
    var state: EditorCaretState?
    var canUndo = false
    var canRedo = false
    /// The formatting keyboard is up in place of the system keyboard.
    var formatting = false

    @ObservationIgnored var run: (EditCommand) -> Void = { _ in }
    @ObservationIgnored var undo: () -> Void = {}
    @ObservationIgnored var redo: () -> Void = {}
    /// Aa: the formatting keyboard, or back to the system keyboard.
    @ObservationIgnored var setFormatting: (Bool) -> Void = { _ in }
    @ObservationIgnored var hideKeyboard: () -> Void = {}
    /// The Link key, ⌘K and the link chip's Edit: the link sheet.
    @ObservationIgnored var editLink: () -> Void = {}
    /// The link chip's Open, for a web link: the URL leaves for the browser.
    @ObservationIgnored var open: (URL) -> Void = { UIApplication.shared.open($0) }

    /// The command's button shows on: the caret is in that format, on that
    /// kind of line, in that block.
    func isOn(_ command: EditCommand) -> Bool {
        guard let state else { return false }
        return switch command {
        case .format(let format): state.formats.contains(format)
        case .list: state.list
        case .task: state.task != nil
        case .turnInto(let kind): state.block == kind
        case .table(.align(let align)): state.cell?.align == align
        case .indent, .outdent, .rule, .insertTable, .table, .moveInTable, .lineBreak: false
        }
    }

    /// The command can apply at the caret: inline formats outside code,
    /// indent under an item, outdent with an indent; blocks outside tables;
    /// table controls in a drawn table's cell, but no row above or deleted
    /// in the header and no last column deleted.
    func isEnabled(_ command: EditCommand) -> Bool {
        guard let state else { return false }
        return switch command {
        case .format: state.canFormat
        case .indent: state.canIndent
        case .outdent: state.canOutdent
        case .list, .task, .turnInto: !state.inTable
        case .rule, .insertTable: state.canFormat && !state.inTable
        case .table(let command): state.cell.map { Self.applies(command, in: $0) } ?? false
        case .moveInTable, .lineBreak: state.cell != nil
        }
    }

    /// Whether a table control does something in `cell`.
    static func applies(_ command: EditorTableCommand, in cell: EditorTableCell) -> Bool {
        switch command {
        case .insertRowAbove, .deleteRow: cell.row > 0
        case .deleteColumn: cell.columns > 1
        case .insertRowBelow, .duplicateRow, .insertColumnLeft, .insertColumnRight, .align, .style, .deleteTable: true
        }
    }

    /// The table controls show in place of the list and task buttons
    /// (ADR 0110): the caret is in a drawn table's cell.
    var inCell: Bool { state?.cell != nil }

    /// What VoiceOver says the task button's line is.
    var taskValue: String? {
        switch state?.task {
        case .some(true): String(localized: "Checked")
        case .some(false): String(localized: "Not checked")
        case nil: nil
        }
    }

    /// The link chip's link, when its URL opens in the browser.
    var webURL: URL? {
        guard let link = state?.link, link.web else { return nil }
        return URL(string: link.url)
    }
}

/// The link sheet's fields (ADR 0109), and what Done or Cancel does.
struct LinkDraft: Identifiable {
    let id = UUID()
    var text: String
    var url: String
    /// A link at the caret is edited: an empty URL removes it.
    let editing: Bool
    /// Done, with the fields as typed.
    let finish: @MainActor (_ text: String, _ url: String) -> Void
    /// The sheet went, after Done or not.
    let closed: @MainActor () -> Void
}
