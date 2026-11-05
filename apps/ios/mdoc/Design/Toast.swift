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

    var body: some View {
        HStack(spacing: Space.s) {
            Text(message.text)
                .font(Typography.chip)
                .lineLimit(2)
            if let title = message.actionTitle, let action = message.action {
                Button(title) {
                    action()
                    dismiss()
                }
                .font(Typography.chip.weight(.semibold))
                .foregroundStyle(Palette.accent)
                .frame(minHeight: Size.target)
            }
        }
        .padding(.horizontal, Space.m)
        .frame(minHeight: Size.target)
        .background(.regularMaterial, in: Capsule())
        .shadow(color: .black.opacity(0.12), radius: 12, y: 4)
        .accessibilityElement(children: .combine)
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
                        try? await Task.sleep(for: .seconds(seconds))
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

#Preview("Toast, dark") {
    Toast(message: ToastMessage(text: "Undid Replace · Riverside Holdings", actionTitle: "Redo", action: {}))
        .padding()
        .preferredColorScheme(.dark)
}
