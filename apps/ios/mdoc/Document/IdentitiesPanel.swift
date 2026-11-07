import SwiftUI

/// Identities with what its rows open: the bar's full view on a name, a
/// focused review, and Same as…'s searchable picker. The iPad inspector and
/// the iPhone drawer show the same panel.
struct IdentitiesPanel: View {
    @Bindable var model: DocumentViewModel
    /// Beside the text (iPad): nothing to close.
    var isRegular: Bool
    @State private var sameAsSource: IdentityID?

    var body: some View {
        IdentitiesView(
            model: model,
            onOpen: { id in
                if !isRegular { model.showsIdentities = false }
                model.openIdentity(id)
            },
            onFocus: { focus in
                if !isRegular { model.showsIdentities = false }
                model.beginReview(focus: focus)
            },
            onPickSameAs: { sameAsSource = $0 },
            onClose: isRegular ? nil : { withAnimation(Motion.snappy) { model.showsIdentities = false } })
        // In the drawer the document's own toast is under the panel.
        .modifier(DrawerToast(model: model, enabled: !isRegular && model.showsIdentities))
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
/// the mirror of the sidebar (2026-11-06 grill, Q8). Without a document
/// the screen shows as it is.
struct IdentitiesDrawer<Content: View>: View {
    @Bindable var app: AppModel
    @ViewBuilder var content: () -> Content

    var body: some View {
        if let id = app.selection, let model = app.documentModel(for: id) {
            SideDrawer(side: .trailing, isOpen: Bindable(model).showsIdentities, closeLabel: "Close Identities") {
                IdentitiesPanel(model: model, isRegular: false)
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
