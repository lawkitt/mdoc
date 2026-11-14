import MdocFFI
import SwiftUI

/// Identities' rows (docs/mobile-ui.md, Identities): the line a name shows,
/// what a tap, a swipe and a long press do, and in Select the ≡ handle.
extension IdentitiesView {
    func row(_ identity: Identity) -> some View {
        let selected = selection.contains(identity.id)
        let line = rowLine(identity, selected: selected)
            .padding(.vertical, Space.xxs)
            .contentShape(Rectangle())
            .onTapGesture {
                if selecting {
                    if selected { selection.remove(identity.id) } else { selection.insert(identity.id) }
                    Haptics.selection()
                } else {
                    onOpen(identity.id)
                }
            }
            .accessibilityElement(children: .combine)
            .accessibilityLabel(Text(Self.spoken(identity)))
            .accessibilityAddTraits(selecting ? (selected ? [.isSelected, .isButton] : .isButton) : .isButton)
            .accessibilityHint(Text(selecting ? "Selects it." : "Opens it in the review bar."))
            .accessibilityAction(named: Text(identity.state == .replaced ? "Keep all" : "Replace all")) {
                deciding([identity.id]) { model.toggle(identity) }
            }
            .accessibilityAction(named: Text("Review this identity")) { onFocus(.identity(identity.id)) }
            .accessibilityAction(named: Text("Same as…")) { onPickSameAs(identity.id) }
            .accessibilityIdentifier("identity-row")
        // One line in and out of Select: as two branches its text would
        // cross-fade, drawn twice and offset, as Select's ticks slid in
        // (ADR 0116). In Select the ≡ handle takes the state mark's place:
        // an element of its own beside the row's, for VoiceOver.
        return HStack(spacing: Space.s) {
            line
            if selecting {
                dragHandle(identity)
            }
        }
        .accessibilityElement(children: .contain)
        .modifier(DragMergeRow(
            id: identity.id, lifted: drag?.dragged == identity.id, targeted: drag?.target == identity.id,
            tracking: dragTracking))
        .listRowBackground(selected ? Palette.accent.opacity(0.08) : Color.clear)
        .listRowInsets(EdgeInsets(top: Space.xxs, leading: Space.m, bottom: Space.xxs, trailing: Space.xs))
        // Left only: Replace outermost, then Same as… and Keep. No full
        // swipe: with one, swiping a row with its actions showing back to
        // the right ran Replace (UIKit), and Keep sends the original to the
        // AI (ADR 0106). A swipe right is the drawer's close (ADR 0115). In
        // Select none, nor the long press's menu: a row there ticks, and
        // its ≡ handle drags.
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            if !selecting {
                Button("Replace") { decide(identity, .replace) }
                    .tint(Palette.swipeAccent)
                Button("Same as…") { onPickSameAs(identity.id) }
                    .tint(.indigo)
                Button("Keep") { decide(identity, .keep) }
                    .tint(.gray)
            }
        }
        .contextMenu {
            if !selecting {
                Button {
                    onFocus(.identity(identity.id))
                } label: {
                    Label("Review this identity", systemImage: "rectangle.stack")
                }
                Button {
                    decide(identity, .replace)
                } label: {
                    Label("Replace all", systemImage: "eye.slash")
                }
                Button {
                    decide(identity, .keep)
                } label: {
                    Label("Keep all", systemImage: "eye")
                }
                Button {
                    onPickSameAs(identity.id)
                } label: {
                    Label("Same as…", systemImage: "link")
                }
            }
        }
    }

    /// A row's line: Select's tick, the category, the name, its alias and
    /// other spellings ("Same as PERSON_1" while a drag is over it), and the
    /// state mark outside Select. The lifted copy draws the same line, with
    /// `hint` in the alias line's place.
    func rowLine(_ identity: Identity, selected: Bool, hint: String? = nil) -> some View {
        let dropHint = hint ?? dropHint(on: identity)
        return HStack(spacing: Space.s) {
            if selecting {
                Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                    .font(.title3)
                    .foregroundStyle(selected ? Palette.accent : Color.secondary)
                    .accessibilityHidden(true)
            }
            Image(systemName: identity.category.symbol)
                .foregroundStyle(identity.category.color)
                .frame(width: 24)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                // Plain word wrap: iOS 26 hyphenates a Text mid-word
                // ("example.-com").
                UnhyphenatedText(identity.primaryWording)
                HStack(spacing: Space.xs) {
                    UnhyphenatedText(identity.alias, style: .footnote, monospaced: true, color: .secondaryLabel)
                        .fixedSize()
                    if identity.mentionCount > 1 {
                        Text("\(identity.mentionCount)×")
                            .font(Typography.caption)
                            .foregroundStyle(Color(uiColor: .secondaryLabel))
                            .fixedSize()
                    }
                    // Its other spellings, in grey.
                    let others = identity.variants.filter { $0 != identity.primaryWording }
                    if !others.isEmpty {
                        Text(verbatim: "· " + others.joined(separator: ", "))
                            .font(Typography.caption)
                            .foregroundStyle(Color(uiColor: .secondaryLabel))
                            .lineLimit(1)
                    }
                }
                // A drag over the row: "Same as PERSON_1" in its place, as
                // wide as the row, not the alias.
                .frame(maxWidth: .infinity, alignment: .leading)
                .opacity(dropHint == nil ? 1 : 0)
                .overlay(alignment: .leading) {
                    if let dropHint { DropHintLabel(text: dropHint) }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if !selecting {
                Button {
                    deciding([identity.id]) { model.toggle(identity) }
                } label: {
                    StateButtonLabel(state: identity.state, tint: identity.category.color)
                        .id(identity.state)
                        .transition(markTransition)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text(ReviewDetails.stateName(identity.state)))
                .accessibilityHint(Text(identity.state == .replaced ? "Keeps every mention." : "Replaces every mention."))
                .accessibilityIdentifier("identity-state")
            }
        }
    }
}
