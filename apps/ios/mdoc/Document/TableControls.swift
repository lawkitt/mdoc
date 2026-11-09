import MdocFFI
import SwiftUI

/// The formatting keyboard's page in a table's cell (ADR 0109), as Bear's
/// table keyboard: rows (insert above or below, duplicate, delete),
/// columns (insert left or right, delete), the column's alignment, moves
/// between cells, the formats that work in a cell, delete table and the
/// key back to the system keyboard. Each control is one editor-core table
/// command, one edit and one undo step; a move edits nothing.
struct TableControls: View {
    let model: EditToolbarModel

    /// A table control: its command, symbol, name and identifier.
    typealias Control = (command: EditCommand, symbol: String, label: LocalizedStringKey, id: String)

    static let rows: [Control] = [
        (.table(.insertRowAbove), "arrow.up.to.line", "Row above", "table-row-above"),
        (.table(.insertRowBelow), "arrow.down.to.line", "Row below", "table-row-below"),
        (.table(.duplicateRow), "plus.square.on.square", "Duplicate row", "table-duplicate-row"),
        (.table(.deleteRow), "minus.rectangle", "Delete row", "table-delete-row"),
    ]

    static let columns: [Control] = [
        (.table(.insertColumnLeft), "arrow.left.to.line", "Column left", "table-column-left"),
        (.table(.insertColumnRight), "arrow.right.to.line", "Column right", "table-column-right"),
        (.table(.deleteColumn), "minus.square", "Delete column", "table-delete-column"),
    ]

    static let aligns: [Control] = [
        (.table(.align(align: .left)), "text.alignleft", "Align left", "table-align-left"),
        (.table(.align(align: .center)), "text.aligncenter", "Align center", "table-align-center"),
        (.table(.align(align: .right)), "text.alignright", "Align right", "table-align-right"),
    ]

    static let moves: [Control] = [
        (.moveInTable(.previous), "arrow.backward", "Previous cell", "table-previous-cell"),
        (.moveInTable(.next), "arrow.forward", "Next cell", "table-next-cell"),
        (.moveInTable(.up), "arrow.up", "Cell above", "table-cell-above"),
        (.moveInTable(.down), "arrow.down", "Cell below", "table-cell-below"),
    ]

    var body: some View {
        KeySection(title: Text("Rows")) { keys(Self.rows) }
        KeySection(title: Text("Columns")) { keys(Self.columns) }
        KeySection(title: Text("Align column")) { keys(Self.aligns) }
        KeySection(title: Text("Move")) { keys(Self.moves) }
        KeySection(title: Text("Format")) { FormatKeys(model: model) }
        KeySection(title: Text("Table")) {
            CommandKey(
                model: model, command: .table(.deleteTable), symbol: "trash", label: Text("Delete table"),
                id: "table-delete")
            KeyboardKey(model: model)
        }
    }

    private func keys(_ controls: [Control]) -> some View {
        ForEach(controls, id: \.id) { control in
            CommandKey(model: model, command: control.command, symbol: control.symbol, label: Text(control.label), id: control.id)
        }
    }
}
