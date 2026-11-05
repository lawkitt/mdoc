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
                    } footer: {
                        if let first = matches.first, model.candidate(covering: first) == nil {
                            Text("Hide name marks it, and every exact repeat, as one name to review.")
                        }
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
                ToolbarItem(placement: .confirmationAction) {
                    Button("Hide name") {
                        guard let first = matches.first else { return }
                        dismiss()
                        model.markSelection(first)
                    }
                    .disabled(matches.isEmpty || matches.first.flatMap { model.candidate(covering: $0) } != nil)
                    .accessibilityIdentifier("find-hide-name")
                }
            }
            .onAppear { focused = true }
        }
        .presentationDetents([.medium, .large])
    }
}
