import SwiftUI

/// The Cases page (the sidebar's Cases row): each case with its colour and
/// how many documents it holds; a tap shows its documents.
struct CasesView: View {
    @Bindable var app: AppModel
    @Environment(\.showSidebar) private var showSidebar

    private var library: LibraryModel { app.library }

    var body: some View {
        List {
            if library.cases.isEmpty {
                ContentUnavailableView(
                    "No cases yet",
                    systemImage: "briefcase",
                    description: Text("Each document you add starts a case, or joins one you choose."))
                    .listRowBackground(Color.clear)
            }
            ForEach(library.cases) { summary in
                Button {
                    app.openCase(summary.id)
                } label: {
                    HStack(spacing: Space.s) {
                        CaseDot(colour: summary.colour)
                        VStack(alignment: .leading, spacing: Space.xxs) {
                            Text(summary.name)
                                .font(Typography.body.weight(.medium))
                                .foregroundStyle(.primary)
                            Text(count(summary))
                                .font(Typography.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 0)
                        Image(systemName: "chevron.right")
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(.tertiary)
                            .accessibilityHidden(true)
                    }
                    .frame(minHeight: Size.target)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .combine)
                .contextMenu {
                    Button {
                        app.navigator.present(.renameCase(summary.id))
                    } label: {
                        Label("Rename…", systemImage: "pencil")
                    }
                }
                .swipeActions {
                    Button("Rename") { app.navigator.present(.renameCase(summary.id)) }
                        .tint(Palette.swipeAccent)
                }
                .accessibilityAction(named: Text("Rename")) { app.navigator.present(.renameCase(summary.id)) }
                .accessibilityIdentifier("case-row")
            }
        }
        .navigationTitle("Cases")
        .toolbar {
            if let showSidebar {
                SidebarButton(action: showSidebar)
            }
        }
    }

    private func count(_ summary: CaseSummary) -> String {
        let documents = library.documents.filter { $0.caseID == summary.id }
        let toReview = documents.filter { $0.phase == .reviewing }.count
        let base = documents.count == 1 ? String(localized: "1 document") : String(localized: "\(documents.count) documents")
        return toReview > 0 ? base + " · " + String(localized: "\(toReview) to review") : base
    }
}
