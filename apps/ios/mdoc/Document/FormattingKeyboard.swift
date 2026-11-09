import MdocFFI
import SwiftUI
import UIKit

/// Edit mode's formatting keyboard (ADR 0109), which Aa swaps in for the
/// system keyboard as Bear and Taio do: Turn into (text, three headings,
/// quote, code block, callout), strikethrough, inline code, highlight,
/// link, rule and table, and a key back to the system keyboard. The caret's
/// block shows selected. It scrolls when the text size leaves it no room.
struct FormattingKeyboard: View {
    let model: EditToolbarModel

    @ScaledMetric(relativeTo: .body) private var keyWidth: CGFloat = 64

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
            VStack(alignment: .leading, spacing: Space.s) {
                // The table slice (ADR 0109) shows table controls here while
                // the caret is in a table (`model.state?.inTable`); until
                // then the block keys are off there.
                section(Text("Turn into")) {
                    ForEach(Self.kinds, id: \.3) { kind, symbol, label, id in
                        key(.turnInto(kind), symbol: symbol, label: Text(label), id: id)
                    }
                }
                section(Text("Format")) {
                    key(.format(.strikethrough), symbol: "strikethrough", label: Text("Strikethrough"), id: "format-strikethrough")
                    key(.format(.code), symbol: "chevron.left.forwardslash.chevron.right", label: Text("Inline code"), id: "format-code")
                    key(.format(.highlight), symbol: "highlighter", label: Text("Highlight"), id: "format-highlight")
                    FormattingKey(symbol: "link", label: Text("Link"), id: "format-link", on: model.state?.link != nil) {
                        model.editLink()
                    }
                }
                section(Text("Insert")) {
                    key(.rule, symbol: "minus", label: Text("Rule"), id: "insert-rule")
                    key(.table, symbol: "tablecells", label: Text("Table"), id: "insert-table")
                    FormattingKey(
                        symbol: "keyboard", label: Text("Keyboard"), id: "formatting-keyboard-return",
                        hint: Text("Brings back the keyboard.")
                    ) {
                        model.setFormatting(false)
                    }
                }
            }
            .padding(Space.s)
        }
        .scrollBounceBehavior(.basedOnSize)
        .accessibilityIdentifier("formatting-keyboard")
    }

    private func section(_ title: Text, @ViewBuilder keys: () -> some View) -> some View {
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

    private func key(_ command: EditCommand, symbol: String, label: Text, id: String) -> some View {
        FormattingKey(
            symbol: symbol, label: label, id: id, on: model.isOn(command), enabled: model.isEnabled(command)
        ) {
            model.run(command)
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
            VStack(spacing: Space.xxs) {
                Image(systemName: symbol)
                    .font(.body.weight(.medium))
                label
                    .font(.caption2)
                    .lineLimit(2)
                    .multilineTextAlignment(.center)
                    .minimumScaleFactor(0.8)
            }
            .foregroundStyle(on ? Color.onAccent : enabled ? Color.primary : Color.secondary)
            .padding(.vertical, Space.xs)
            .padding(.horizontal, Space.xxs)
            .frame(maxWidth: .infinity, minHeight: Size.target * 1.2)
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
