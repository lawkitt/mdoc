import SwiftUI

/// Identities with what its rows open: the bar's full view on a name, a
/// focused review, and Same as…'s searchable picker. The iPad inspector and
/// the iPhone drawer show the same panel.
struct IdentitiesPanel: View {
    @Bindable var model: DocumentViewModel
    @State private var sameAsSource: IdentityID?
    /// The drawer's own close, which slides it shut.
    @Environment(\.closeDrawer) private var closeDrawer

    /// In the drawer: a row closes it, and it has its own close button.
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
            onPickSameAs: { sameAsSource = $0 },
            onClose: inDrawer ? closeDrawer ?? { model.showIdentities(false) } : nil)
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

/// iPhone: the open document's Identities as a drawer from the right edge,
/// the mirror of the sidebar (2026-11-06 grill, Q8). Without a document,
/// or in a regular window, the screen shows as it is.
struct IdentitiesDrawer<Content: View>: View {
    let model: DocumentViewModel?
    let layout: DocumentLayout
    @ViewBuilder var content: () -> Content

    var body: some View {
        if let model, layout.identities(hasRightPane: model.hasRightPane) == .drawer {
            // The drawer, not the wish: a wide window that narrows keeps
            // the text in view (#674).
            SideDrawer(
                side: .trailing,
                isOpen: Binding(get: { model.identitiesDrawerOpen }, set: { model.showIdentities($0) }),
                closeLabel: "Close Identities"
            ) {
                IdentitiesPanel(model: model)
            } content: {
                content()
            }
        } else {
            content()
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
