import MdocFFI
import SwiftUI
import UIKit

/// Edit mode's formatting keyboard (ADR 0109), which Aa swaps in for the
/// system keyboard as Bear and Taio do: Turn into (text, three headings,
/// quote, code block, callout), strikethrough, inline code, highlight,
/// link, rule and table; Aa switches back (ADR 0110). The caret's block
/// shows selected. In a table's cell it is the same keyboard: the formats
/// work there, the blocks are off. It scrolls when the text size leaves it
/// no room.
struct FormattingKeyboard: View {
    let model: EditToolbarModel

    /// The turn-into kinds, in the keyboard's order.
    static let kinds: [(EditorBlockKind, String, LocalizedStringKey, String)] = [
        (.text, "text.alignleft", "Text", "turn-text"),
        (.heading1, "1.square", "Heading 1", "turn-heading1"),
        (.heading2, "2.square", "Heading 2", "turn-heading2"),
        (.heading3, "3.square", "Heading 3", "turn-heading3"),
        (.quote, "text.quote", "Quote", "turn-quote"),
        (.codeBlock, "curlybraces", "Code block", "turn-code-block"),
        (.callout, "exclamationmark.bubble", "Callout", "turn-callout"),
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Space.xs) {
                keys
            }
            .padding(.horizontal, Space.s)
            .padding(.vertical, Space.xs)
        }
        .scrollBounceBehavior(.basedOnSize)
        .accessibilityIdentifier("formatting-keyboard")
    }

    @ViewBuilder private var keys: some View {
        KeySection(title: Text("Turn into")) {
            ForEach(Self.kinds, id: \.3) { kind, symbol, label, id in
                CommandKey(model: model, command: .turnInto(kind), symbol: symbol, label: Text(label), id: id)
            }
        }
        KeySection(title: Text("Format")) {
            FormatKeys(model: model)
        }
        KeySection(title: Text("Insert")) {
            CommandKey(model: model, command: .rule, symbol: "minus", label: Text("Rule"), id: "insert-rule")
            CommandKey(model: model, command: .insertTable, symbol: "tablecells", label: Text("Table"), id: "insert-table")
        }
    }
}

/// A titled group of keys, as many to a row as fit.
struct KeySection<Keys: View>: View {
    let title: Text
    @ViewBuilder let keys: () -> Keys

    @ScaledMetric(relativeTo: .body) private var keyWidth: CGFloat = 64

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xxs) {
            title
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
                .accessibilityAddTraits(.isHeader)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: keyWidth), spacing: Space.xs)], spacing: Space.xs) {
                keys()
            }
        }
    }
}

/// A key that runs one editor command, on where it applies and off where
/// it cannot.
struct CommandKey: View {
    let model: EditToolbarModel
    let command: EditCommand
    let symbol: String
    let label: Text
    let id: String

    var body: some View {
        FormattingKey(
            symbol: symbol, label: label, id: id, on: model.isOn(command), enabled: model.isEnabled(command)
        ) {
            model.run(command)
        }
    }
}

/// Strikethrough, inline code, highlight and link: in text and in cells.
struct FormatKeys: View {
    let model: EditToolbarModel

    var body: some View {
        CommandKey(
            model: model, command: .format(.strikethrough), symbol: "strikethrough", label: Text("Strikethrough"),
            id: "format-strikethrough")
        CommandKey(
            model: model, command: .format(.code), symbol: "chevron.left.forwardslash.chevron.right",
            label: Text("Inline code"), id: "format-code")
        CommandKey(
            model: model, command: .format(.highlight), symbol: "highlighter", label: Text("Highlight"),
            id: "format-highlight")
        FormattingKey(symbol: "link", label: Text("Link"), id: "format-link", on: model.state?.link != nil) {
            model.editLink()
        }
    }
}

/// One formatting key: a symbol over its name, on a key the size of the
/// system keyboard's. On, it is ink and VoiceOver hears it selected.
struct FormattingKey: View {
    let symbol: String
    let label: Text
    let id: String
    var on = false
    var enabled = true
    var hint: Text?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 0) {
                Image(systemName: symbol)
                    .font(.callout.weight(.medium))
                label
                    .font(.caption2)
                    .lineLimit(2)
                    .multilineTextAlignment(.center)
                    .minimumScaleFactor(0.8)
            }
            .foregroundStyle(on ? Color.onAccent : enabled ? Color.primary : Color.secondary)
            .padding(.vertical, Space.xxs)
            .padding(.horizontal, Space.xxs)
            .frame(maxWidth: .infinity, minHeight: Size.target)
            .background(
                on ? Color.ink : Color(uiColor: .formattingKey),
                in: .rect(cornerRadius: Radius.control, style: .continuous))
            .contentShape(.rect(cornerRadius: Radius.control))
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityHint(hint ?? Text(""))
        .accessibilityAddTraits(on ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier(id)
    }
}

extension UIColor {
    /// A formatting key on the keyboard's background, as the system's keys:
    /// white in light mode, raised grey in dark.
    static let formattingKey = UIColor { traits in
        traits.userInterfaceStyle == .dark ? .systemGray3 : .systemBackground
    }
}

/// The formatting keyboard as the text view's `inputView`: the keyboard's
/// own background, as tall as the system keyboard was.
final class FormattingKeyboardHost: UIInputView {
    private let hosting: UIHostingController<FormattingKeyboard>

    init(model: EditToolbarModel, height: CGFloat) {
        hosting = UIHostingController(rootView: FormattingKeyboard(model: model))
        super.init(frame: CGRect(x: 0, y: 0, width: 0, height: height), inputViewStyle: .keyboard)
        allowsSelfSizing = true
        hosting.view.backgroundColor = .clear
        hosting.view.translatesAutoresizingMaskIntoConstraints = false
        addSubview(hosting.view)
        let tall = heightAnchor.constraint(equalToConstant: height)
        tall.priority = .defaultHigh
        NSLayoutConstraint.activate([
            hosting.view.leadingAnchor.constraint(equalTo: leadingAnchor),
            hosting.view.trailingAnchor.constraint(equalTo: trailingAnchor),
            hosting.view.topAnchor.constraint(equalTo: topAnchor),
            hosting.view.bottomAnchor.constraint(equalTo: safeAreaLayoutGuide.bottomAnchor),
            tall,
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }
}
