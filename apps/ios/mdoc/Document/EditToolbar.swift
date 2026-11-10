import MdocFFI
import SwiftUI
import UIKit

/// Edit mode's toolbar above the keyboard (ADR 0109): a Liquid Glass
/// capsule of undo, redo, bold, italic, list, task, indent and outdent,
/// then Aa and hide keyboard as glass circles that always show. With the
/// caret in a table's cell, row below, column right and the table's ⋯ menu
/// take the list and task buttons' place (ADR 0110). A button is on while the caret is in what it makes (bold inside `**…**`, list on a
/// list line) and off where it cannot apply. Every button is a 44 pt
/// target; the capsule scrolls sideways when the phone or the text size
/// leaves it no room. With the caret in a link, a chip above it offers
/// Open and Edit.
struct EditToolbar: View {
    let model: EditToolbarModel

    var body: some View {
        GlassEffectContainer(spacing: Space.xs) {
            VStack(alignment: .leading, spacing: Space.xs) {
                if let link = model.state?.link {
                    LinkChip(link: link, model: model)
                }
                HStack(spacing: Space.xs) {
                    ScrollViewReader { proxy in
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 0) {
                                buttons
                            }
                        }
                        .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
                        .onChange(of: model.inCell) { _, inCell in follow(inCell, with: proxy) }
                    }
                    .glassEffect(.regular, in: .capsule)
                    ToolbarKey(
                        symbol: "textformat", label: Text("Formatting"), id: "toolbar-formatting", on: model.formatting,
                        hint: Text("Shows formatting keys in place of the keyboard.")
                    ) {
                        model.setFormatting(!model.formatting)
                    }
                    .glassEffect(.regular.interactive(), in: .circle)
                    ToolbarKey(
                        symbol: "keyboard.chevron.compact.down", label: Text("Hide keyboard"), id: "toolbar-hide-keyboard"
                    ) { model.hideKeyboard() }
                        .glassEffect(.regular.interactive(), in: .circle)
                }
            }
        }
        .padding(.horizontal, Space.xs)
        .padding(.vertical, Space.xxs)
    }

    @ViewBuilder
    private var buttons: some View {
        ToolbarKey(symbol: "arrow.uturn.backward", label: Text("Undo"), id: "toolbar-undo", enabled: model.canUndo) {
            model.undo()
        }
        .id(Self.undoID)
        ToolbarKey(symbol: "arrow.uturn.forward", label: Text("Redo"), id: "toolbar-redo", enabled: model.canRedo) {
            model.redo()
        }
        command(.format(.bold), symbol: "bold", label: Text("Bold"), id: "toolbar-bold")
        command(.format(.italic), symbol: "italic", label: Text("Italic"), id: "toolbar-italic")
        if model.inCell {
            // The table controls (ADR 0110): in a cell they take the list
            // and task buttons' place; indent and outdent, which never
            // apply in a cell, give theirs too.
            command(.table(.insertRowBelow), symbol: "arrow.down.to.line", label: Text("Row below"), id: "table-row-below")
            command(
                .table(.insertColumnRight), symbol: "arrow.right.to.line", label: Text("Column right"),
                id: "table-column-right")
            TableMenu(model: model)
                .id(Self.tableMenuID)
        } else {
            command(.list, symbol: "list.bullet", label: Text("Bulleted list"), id: "toolbar-list")
            command(.task, symbol: "checklist", label: Text("Task"), id: "toolbar-task", value: model.taskValue)
            command(.indent, symbol: "increase.indent", label: Text("Indent"), id: "toolbar-indent")
            command(.outdent, symbol: "decrease.indent", label: Text("Outdent"), id: "toolbar-outdent")
        }
    }

    /// The scroll ids of the capsule's first button and of ⋯.
    static let undoID = "toolbar-undo"
    static let tableMenuID = "table-more"

    /// Entering a cell scrolls the capsule to its end, so ⋯ shows whole on
    /// a phone too narrow for every button; leaving one scrolls back to
    /// Undo.
    private func follow(_ inCell: Bool, with proxy: ScrollViewProxy) {
        let animation: Animation? = UIAccessibility.isReduceMotionEnabled ? nil : .default
        withAnimation(animation) {
            if inCell {
                proxy.scrollTo(Self.tableMenuID, anchor: .trailing)
            } else {
                proxy.scrollTo(Self.undoID, anchor: .leading)
            }
        }
    }

    private func command(
        _ command: EditCommand, symbol: String, label: Text, id: String, value: String? = nil
    ) -> some View {
        ToolbarKey(
            symbol: symbol, label: label, id: id, on: model.isOn(command), enabled: model.isEnabled(command), value: value
        ) {
            model.run(command)
        }
    }
}

/// The table's ⋯ menu in the toolbar (ADR 0110): Row above, Column left,
/// Delete row, Delete column, Align column (the column's alignment
/// checked), then Delete table. Each item is one editor-core table
/// command; one that cannot apply there (a row above or deleted in the
/// header, the last column deleted) is off.
struct TableMenu: View {
    let model: EditToolbarModel

    @ScaledMetric(relativeTo: .body) private var side = Size.target

    var body: some View {
        Menu {
            item(.insertRowAbove, symbol: "arrow.up.to.line", label: Text("Row above"), id: "table-row-above")
            item(.insertColumnLeft, symbol: "arrow.left.to.line", label: Text("Column left"), id: "table-column-left")
            item(.deleteRow, symbol: "minus.rectangle", label: Text("Delete row"), id: "table-delete-row")
            item(.deleteColumn, symbol: "minus.square", label: Text("Delete column"), id: "table-delete-column")
            Menu {
                Picker(selection: align) {
                    Text("Left").tag(MarkdownAlign.left)
                    Text("Center").tag(MarkdownAlign.center)
                    Text("Right").tag(MarkdownAlign.right)
                } label: {
                    Text("Align column")
                }
                .pickerStyle(.inline)
            } label: {
                Label { Text("Align column") } icon: { Image(systemName: "text.aligncenter") }
            }
            .accessibilityIdentifier("table-align")
            Divider()
            item(.deleteTable, symbol: "trash", label: Text("Delete table"), id: "table-delete", role: .destructive)
        } label: {
            Image(systemName: "ellipsis")
                .font(.body.weight(.medium))
                .foregroundStyle(Color.primary)
                .frame(minWidth: side, minHeight: side)
                .contentShape(Rectangle())
        }
        .menuOrder(.fixed)
        .buttonStyle(.plain)
        .accessibilityLabel(Text("Table"))
        .accessibilityHint(Text("Rows, columns, alignment and delete table."))
        .accessibilityIdentifier("table-more")
    }

    /// The caret's column's alignment; choosing another sets it.
    private var align: Binding<MarkdownAlign> {
        Binding(
            get: { model.state?.cell?.align ?? .left },
            set: { model.run(.table(.align(align: $0))) })
    }

    private func item(
        _ command: EditorTableCommand, symbol: String, label: Text, id: String, role: ButtonRole? = nil
    ) -> some View {
        Button(role: role) {
            model.run(.table(command))
        } label: {
            Label { label } icon: { Image(systemName: symbol) }
        }
        .disabled(!model.isEnabled(.table(command)))
        .accessibilityIdentifier(id)
    }
}

/// One toolbar button: a symbol on a 44 pt square target that grows with
/// the text. On, it is an ink circle and VoiceOver hears it selected.
struct ToolbarKey: View {
    let symbol: String
    let label: Text
    let id: String
    var on = false
    var enabled = true
    var value: String?
    var hint: Text?
    let action: () -> Void

    @ScaledMetric(relativeTo: .body) private var side = Size.target

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.body.weight(.medium))
                .foregroundStyle(on ? Color.onAccent : enabled ? Color.primary : Color.secondary)
                .frame(minWidth: side, minHeight: side)
                .background { if on { Circle().fill(Color.ink).padding(Space.xxs) } }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityLabel(label)
        .accessibilityValue(value.map { Text($0) } ?? Text(""))
        .accessibilityHint(hint ?? Text(""))
        .accessibilityAddTraits(on ? .isSelected : [])
        .accessibilityIdentifier(id)
    }
}

/// The caret's link (ADR 0109): its address, Open for a web link (after
/// the tap, never on its own) and Edit, which opens the link sheet.
struct LinkChip: View {
    let link: EditorLink
    let model: EditToolbarModel

    var body: some View {
        HStack(spacing: Space.xs) {
            Image(systemName: "link")
                .accessibilityHidden(true)
            Text(verbatim: link.url)
                .lineLimit(1)
                .truncationMode(.middle)
                .foregroundStyle(.secondary)
                .accessibilityLabel(Text("Link to \(link.url)"))
            Button("Open") {
                if let url = model.webURL { model.open(url) }
            }
            .disabled(model.webURL == nil)
            .accessibilityHint(Text("Opens the link in your browser."))
            .accessibilityIdentifier("link-open")
            Button("Edit") { model.editLink() }
                .accessibilityHint(Text("Changes the link's text or address."))
                .accessibilityIdentifier("link-edit")
        }
        .font(.footnote.weight(.medium))
        .buttonStyle(.plain)
        .padding(.horizontal, Space.s)
        .frame(minHeight: Size.target)
        .glassEffect(.regular.interactive(), in: .capsule)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("link-chip")
    }
}

/// The toolbar as the text view's `inputAccessoryView`: a hosting
/// controller that never becomes first responder, so a tap on a button
/// leaves the keyboard and the caret where they are. It is as tall as the
/// toolbar, which grows with the text size and with the link chip.
final class EditToolbarHost: UIView {
    private final class Hosting: UIHostingController<EditToolbar> {
        override var canBecomeFirstResponder: Bool { false }
    }

    private let hosting: Hosting
    private var height: CGFloat = 60

    init(model: EditToolbarModel) {
        hosting = Hosting(rootView: EditToolbar(model: model))
        super.init(frame: CGRect(x: 0, y: 0, width: 0, height: 60))
        autoresizingMask = .flexibleHeight
        backgroundColor = .clear
        hosting.view.backgroundColor = .clear
        hosting.sizingOptions = .intrinsicContentSize
        hosting.view.translatesAutoresizingMaskIntoConstraints = false
        addSubview(hosting.view)
        NSLayoutConstraint.activate([
            hosting.view.leadingAnchor.constraint(equalTo: leadingAnchor),
            hosting.view.trailingAnchor.constraint(equalTo: trailingAnchor),
            hosting.view.topAnchor.constraint(equalTo: topAnchor),
            hosting.view.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    override var canBecomeFirstResponder: Bool { false }

    override var intrinsicContentSize: CGSize {
        CGSize(width: UIView.noIntrinsicMetric, height: height)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        // The toolbar's own height, as the text size and the chip make it.
        let fitted = hosting.sizeThatFits(in: CGSize(width: bounds.width > 0 ? bounds.width : 400, height: .infinity)).height
        if abs(fitted - height) > 0.5 {
            height = fitted
            invalidateIntrinsicContentSize()
        }
    }
}
