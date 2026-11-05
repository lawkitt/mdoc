import SwiftUI

/// A short message after an action, with an optional action such as Undo:
/// "12 replaced · Undo". It sits above the bottom bar, is announced to
/// VoiceOver and goes after a few seconds.
struct ToastMessage: Equatable, Identifiable {
    let id = UUID()
    var text: String
    var actionTitle: String?
    var action: (() -> Void)?

    static func == (lhs: ToastMessage, rhs: ToastMessage) -> Bool { lhs.id == rhs.id }
}

struct Toast: View {
    let message: ToastMessage
    var dismiss: () -> Void = {}
    /// A solid fill instead of the material: snapshots, whose offscreen
    /// render draws materials unevenly.
    var opaque = false

    /// The action button: dismissed first, since the action may show a
    /// toast of its own ("Undid … · Redo"), which must stay.
    func perform() {
        dismiss()
        message.action?()
    }

    var body: some View {
        HStack(spacing: Space.s) {
            Text(message.text)
                .font(Typography.chip)
                .lineLimit(2)
            if let title = message.actionTitle, message.action != nil {
                Button(title, action: perform)
                    .accessibilityIdentifier("toast-action")
                .font(Typography.chip.weight(.semibold))
                .foregroundStyle(Palette.accent)
                .frame(minHeight: Size.target)
            }
        }
        .padding(.horizontal, Space.m)
        .frame(minHeight: Size.target)
        .background {
            if opaque {
                Capsule().fill(Palette.surface)
            } else {
                Capsule().fill(.regularMaterial)
            }
        }
        .shadow(color: .black.opacity(0.12), radius: 12, y: 4)
        // Its action stays a button of its own for VoiceOver and tests.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("toast")
    }
}

/// Shows `message` at the bottom for `seconds`, then clears it.
struct ToastPresenter: ViewModifier {
    @Binding var message: ToastMessage?
    var seconds: Double = 4
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        content.overlay(alignment: .bottom) {
            if let message {
                Toast(message: message) { self.message = nil }
                    .padding(.bottom, Space.xs)
                    .transition(reduceMotion ? .opacity : .move(edge: .bottom).combined(with: .opacity))
                    .task(id: message.id) {
                        UIAccessibility.post(notification: .announcement, argument: message.text)
                        // Longer with an action (Undo, Redo): time to reach it.
                        try? await Task.sleep(for: .seconds(message.action == nil ? seconds : seconds + 2))
                        if self.message?.id == message.id {
                            self.message = nil
                        }
                    }
            }
        }
        .animation(Motion.move(reduced: reduceMotion), value: message)
    }
}

extension View {
    func toast(_ message: Binding<ToastMessage?>, seconds: Double = 4) -> some View {
        modifier(ToastPresenter(message: message, seconds: seconds))
    }
}

#Preview("Toast") {
    VStack(spacing: Space.m) {
        Toast(message: ToastMessage(text: "12 replaced", actionTitle: "Undo", action: {}))
        Toast(message: ToastMessage(text: "Same as Maya Bennett", actionTitle: "Undo", action: {}))
        Toast(message: ToastMessage(text: "All 15 reviewed"))
    }
    .padding()
}

#Preview("Toast, AX5") {
    Toast(message: ToastMessage(text: "12 replaced", actionTitle: "Undo", action: {}))
        .padding()
        .environment(\.dynamicTypeSize, .accessibility5)
}

#Preview("Toast, dark") {
    Toast(message: ToastMessage(text: "Undid Replace · Riverside Holdings", actionTitle: "Redo", action: {}))
        .padding()
        .preferredColorScheme(.dark)
}

#Preview("Toast, matrix") {
    PreviewMatrix {
        Toast(message: ToastMessage(text: "Same as Maya Bennett", actionTitle: "Undo", action: {}))
    }
}
