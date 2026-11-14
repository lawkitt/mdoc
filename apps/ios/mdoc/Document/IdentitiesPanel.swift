import SwiftUI

/// Identities with what its rows open: the bar's full view on a name, a
/// focused review, and Same as…'s searchable picker. The iPad inspector and
/// the iPhone drawer show the same panel.
struct IdentitiesPanel: View {
    @Bindable var model: DocumentViewModel
    @State private var sameAsSource: IdentityID?

    /// In the drawer: a row closes it.
    private var inDrawer: Bool { model.identitiesPlacement == .drawer }

    var body: some View {
        IdentitiesView(
            model: model,
            onOpen: { id in
                if inDrawer { model.showIdentities(false) }
                model.openIdentity(id)
            },
            onFocus: { focus in
                if inDrawer { model.showIdentities(false) }
                model.beginReview(focus: focus)
            },
            onPickSameAs: { sameAsSource = $0 })
        // In the drawer the document's own toast is under the panel.
        .modifier(DrawerToast(model: model, enabled: inDrawer && model.identitiesDrawerOpen))
        .sheet(item: Binding(get: { sameAsSource.map(Source.init) }, set: { if $0 == nil { sameAsSource = nil } })) { source in
            if let identity = model.snapshot.identities.first(where: { $0.id == source.id }) {
                SameAsPicker(
                    source: identity.primaryWording,
                    targets: model.sameAsTargets(forIdentity: identity.id, category: identity.category),
                    categoryLabel: identity.category.label) { target in
                    model.link(identity.id, into: target)
                }
            }
        }
    }

    private struct Source: Identifiable {
        let id: IdentityID
    }
}

extension IdentitiesPanel {
    /// iPhone: the open document's Identities as the drawer's trailing
    /// panel, the mirror of the sidebar (2026-11-06 grill, Q8). Without a
    /// document, or in a regular window, there is none.
    static func drawer(model: DocumentViewModel?, layout: DocumentLayout) -> DrawerPanel<IdentitiesPanel>? {
        // The drawer, not the wish: a wide window that narrows keeps the
        // text in view (#674).
        guard let model, layout.identities(hasRightPane: model.hasRightPane) == .drawer else { return nil }
        return DrawerPanel(
            isOpen: Binding(get: { model.identitiesDrawerOpen }, set: { model.showIdentities($0) }),
            closeLabel: "Close Identities",
            // Hidden while editing: no swipe until Done (ADR 0110). Fixing a
            // name's words, only the edge: a swipe over the table or the text
            // is the fix's (ADR 0115).
            opens: model.isEditing ? .never : model.wordsFix != nil ? .edge : .anywhere
        ) {
            IdentitiesPanel(model: model)
        }
    }
}

/// The document's toast ("Replaced Maya Bennett · Undo") over the drawer.
private struct DrawerToast: ViewModifier {
    @Bindable var model: DocumentViewModel
    let enabled: Bool

    func body(content: Content) -> some View {
        if enabled {
            content.toast($model.toast)
        } else {
            content
        }
    }
}
