import SwiftUI

/// A short message after an action, with an optional action such as Undo:
/// "12 replaced · Undo". It sits above the bottom bar, is announced to
/// VoiceOver and goes after a few seconds; one with an action shows how many
/// are left.
struct ToastMessage: Equatable, Identifiable {
    let id = UUID()
    /// When its time starts: it is shown as it is made.
    let created = Date.now
    var text: String
    var actionTitle: String?
    var action: (() -> Void)?

    static func == (lhs: ToastMessage, rhs: ToastMessage) -> Bool { lhs.id == rhs.id }
}

/// The time a toast's action has left, for its ring.
struct ToastCountdown: Equatable {
    let start: Date
    let duration: TimeInterval

    func remaining(at date: Date) -> TimeInterval {
        min(duration, max(0, duration - date.timeIntervalSince(start)))
    }

    /// The whole seconds to show: 6 from the start until a second has gone.
    func seconds(at date: Date) -> Int {
        Int(remaining(at: date).rounded(.up))
    }

    /// How much of the ring is left, 1 to 0.
    func fraction(at date: Date) -> Double {
        duration > 0 ? remaining(at: date) / duration : 0
    }
}

/// A ring that empties over the toast's time, with the seconds left inside.
/// VoiceOver hears the toast once, not every second.
private struct ToastCountdownRing: View {
    let countdown: ToastCountdown
    @ScaledMetric(relativeTo: .caption) private var size: CGFloat = 26
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.periodic(from: countdown.start, by: reduceMotion ? 1 : 0.05)) { context in
            ZStack {
                Circle().stroke(Palette.accent.opacity(0.2), lineWidth: 2.5)
                Circle()
                    .trim(from: 0, to: countdown.fraction(at: context.date))
                    .stroke(Palette.accent, style: StrokeStyle(lineWidth: 2.5, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                Text(countdown.seconds(at: context.date), format: .number)
                    .font(Typography.caption.weight(.semibold))
                    .monospacedDigit()
                    .minimumScaleFactor(0.6)
                    .padding(3)
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
        .accessibilityIdentifier("toast-countdown")
    }
}

struct Toast: View {
    let message: ToastMessage
    /// How long it stays, for a toast with an action; nil shows no ring.
    var duration: TimeInterval?
    var dismiss: () -> Void = {}
    /// A solid fill instead of the material: snapshots, whose offscreen
    /// render draws materials unevenly.
    var opaque = false

    /// The action button. The action runs before the toast goes, so the
    /// toast's end never acts first (Delete happens when its toast ends,
    /// unless Undo was tapped); a toast the action shows of its own ("Undid
    /// … · Redo") stays, as `dismiss` clears only this message.
    func perform() {
        message.action?()
        dismiss()
    }

    var body: some View {
        HStack(spacing: Space.s) {
            if let duration, message.action != nil {
                ToastCountdownRing(countdown: ToastCountdown(start: message.created, duration: duration))
            }
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

    /// Longer with an action (Undo, Redo): time to reach it.
    static func duration(of message: ToastMessage, seconds: Double) -> TimeInterval {
        message.action == nil ? seconds : seconds + 2
    }

    func body(content: Content) -> some View {
        content.overlay(alignment: .bottom) {
            if let message {
                let duration = Self.duration(of: message, seconds: seconds)
                Toast(message: message, duration: duration) {
                    if self.message?.id == message.id { self.message = nil }
                }
                    .padding(.bottom, Space.xs)
                    .transition(reduceMotion ? .opacity : .move(edge: .bottom).combined(with: .opacity))
                    .task(id: message.id) {
                        UIAccessibility.post(notification: .announcement, argument: message.text)
                        // timing: how long a toast stays, not a wait for an event (ADR 0099)
                        try? await Task.sleep(for: .seconds(duration))
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
        Toast(message: ToastMessage(text: "12 replaced", actionTitle: "Undo", action: {}), duration: 6)
        Toast(message: ToastMessage(text: "Same as Maya Bennett", actionTitle: "Undo", action: {}), duration: 6)
        Toast(message: ToastMessage(text: "All 15 reviewed"))
    }
    .padding()
}

#Preview("Toast, AX5") {
    Toast(message: ToastMessage(text: "12 replaced", actionTitle: "Undo", action: {}), duration: 6)
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
