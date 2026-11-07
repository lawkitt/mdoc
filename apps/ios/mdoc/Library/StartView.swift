import SwiftUI

/// Start (docs/mobile-ui.md, Sidebar): the main area when no document is
/// open. The setup card while models are missing, the document to go back
/// to, the three ways in, and Try a sample. Lists live in the sidebar.
struct StartView: View {
    @Bindable var app: AppModel
    @Environment(\.showSidebar) private var showSidebar

    var body: some View {
        ScrollView {
            VStack(spacing: Space.l) {
                if app.setup.showsCard {
                    SetupCard(model: app.setup)
                }
                if let document = app.library.continueDocument {
                    continueCard(document)
                }
                AddSection(app: app)
            }
            .padding(Space.m)
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
        }
        .background(Palette.background)
        .navigationTitle("mdoc")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if let showSidebar {
                SidebarButton(action: showSidebar)
            }
        }
        .accessibilityIdentifier("start")
    }

    /// "Continue · Services agreement · 3 left".
    private func continueCard(_ document: DocumentSummary) -> some View {
        Button {
            app.open(document.id)
        } label: {
            HStack(spacing: Space.s) {
                IconTile(systemImage: "list.bullet.rectangle", color: Palette.toReview, tinted: true, size: 40)
                VStack(alignment: .leading, spacing: Space.xxs) {
                    Text("Continue")
                        .font(Typography.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Text(document.title)
                        .font(Typography.body.weight(.medium))
                        .foregroundStyle(.primary)
                        .lineLimit(2)
                    StatusPill(Text("\(document.pendingCount) left"), systemImage: "exclamationmark.circle", kind: .toReview)
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.right")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.tertiary)
                    .accessibilityHidden(true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .card(padding: Space.s)
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text("Continue \(document.title), \(document.pendingCount) left"))
        .accessibilityIdentifier("continue-card")
    }
}

/// The ways in, on Start and on the empty Documents page: the three tiles
/// under a heading and, while there are no documents, what mdoc does and
/// Try a sample.
struct AddSection: View {
    let app: AppModel

    private var library: LibraryModel { app.library }

    var body: some View {
        VStack(spacing: Space.l) {
            VStack(spacing: Space.s) {
                Text(library.isEmpty ? "Add a document" : "Add another")
                    .font(.title3.weight(.semibold))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityAddTraits(.isHeader)
                if library.isEmpty {
                    // Below the floor mdoc finds no names: no promise it
                    // does not keep (ADR 0088).
                    Text(library.findsNames
                        ? "A letter, contract or note. mdoc finds the names in it so you can share it with an AI without them."
                        : "A letter, contract or note. Select the names in it to replace them, then share it with an AI.")
                        .font(Typography.caption)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                AddTiles(app: app)
            }
            if library.isEmpty {
                Button("Try a sample") {
                    app.trySample()
                }
                .font(Typography.chip)
                .frame(minHeight: Size.target)
                .accessibilityIdentifier("try-sample")
            }
        }
    }
}
