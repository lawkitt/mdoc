import SwiftUI

/// Find in document (ADR 0098, tier 2): every match of a word in the text,
/// with its context; Hide name marks it, and the core proposes every exact
/// repeat as the same name, so a surname the scan missed is one tap.
struct FindSheet: View {
    @Bindable var model: DocumentViewModel
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @FocusState private var focused: Bool

    private var matches: [SourceRange] { model.matches(of: query) }

    var body: some View {
        NavigationStack {
            List {
                // In the list, not the toolbar: iOS hides toolbar buttons
                // while the search field is active.
                if let unmarked = model.firstUnmarked(of: query) {
                    Section {
                        Button {
                            dismiss()
                            model.markSelection(unmarked)
                        } label: {
                            Label("Hide name", systemImage: "eye.slash")
                                .fontWeight(.semibold)
                        }
                        .accessibilityIdentifier("find-hide-name")
                    } footer: {
                        Text("Marks the first unmarked match, and every exact repeat, as one name to review.")
                    }
                }
                if !query.trimmingCharacters(in: .whitespaces).isEmpty {
                    Section {
                        ForEach(matches.prefix(200), id: \.start) { match in
                            Button {
                                dismiss()
                                model.reveal(match)
                            } label: {
                                Text(model.context(of: match))
                                    .font(Typography.caption)
                                    .foregroundStyle(.primary)
                                    .lineLimit(2)
                            }
                        }
                    } header: {
                        Text(matches.count == 1 ? "1 match" : "\(matches.count) matches")
                    }
                }
            }
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: Text("Find in document"))
            .searchFocused($focused)
            .navigationTitle("Find")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
            }
            // Focus once the sheet is up, not in onAppear: on iOS 26.2,
            // focusing the field while the sheet presents spins the main
            // thread and the app stops responding (#634).
            .task {
                try? await Task.sleep(for: .milliseconds(600))
                focused = true
            }
        }
        .presentationDetents([.medium, .large])
    }
}
