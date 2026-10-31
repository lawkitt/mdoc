import SwiftUI

/// The sheet a tapped candidate opens: its alias, Keep or Replace, and All
/// occurrences or Just this one.
struct CandidateSheet: View {
    let candidate: Candidate
    let identity: Identity?
    let onDone: (Decision, Scope) -> Void
    /// Whether a proposal is left once this one is decided for a scope.
    let hasNext: (Scope) -> Bool
    let onNext: () -> Void
    @Environment(\.dismiss) private var dismiss
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    @State private var replace: Bool
    @State private var scope: Scope = .allOccurrences

    init(
        candidate: Candidate, identity: Identity?, hasNext: @escaping (Scope) -> Bool,
        onDone: @escaping (Decision, Scope) -> Void, onNext: @escaping () -> Void
    ) {
        self.candidate = candidate
        self.identity = identity
        self.hasNext = hasNext
        self.onDone = onDone
        self.onNext = onNext
        _replace = State(initialValue: candidate.state != .kept)
    }

    private var mentionCount: Int { identity?.mentionCount ?? 1 }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Found")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                        UnhyphenatedText(candidate.original)
                    }
                    .accessibilityElement(children: .combine)
                    // At accessibility sizes the value goes under the label;
                    // beside it, "Category" broke over three lines.
                    (dynamicTypeSize.isAccessibilitySize
                        ? AnyLayout(VStackLayout(alignment: .leading, spacing: 4))
                        : AnyLayout(HStackLayout())) {
                        Text("Category")
                        if !dynamicTypeSize.isAccessibilitySize {
                            Spacer()
                        }
                        HStack {
                            Image(systemName: candidate.category.symbol)
                                .foregroundStyle(candidate.category.color)
                                .accessibilityHidden(true)
                            // The icon carries the category's colour; the
                            // words stay in the text colour, which reads (a
                            // category colour as text fails contrast).
                            Text(candidate.category.label)
                        }
                    }
                    .accessibilityElement(children: .combine)
                }
                Section {
                    Picker("Decision", selection: $replace) {
                        Text("Replace").tag(true)
                        Text("Keep").tag(false)
                    }
                    .pickerStyle(.segmented)
                    .accessibilityIdentifier("candidate-decision")
                    if replace && !candidate.alias.isEmpty {
                        LabeledContent("Alias") {
                            Text(candidate.alias)
                                .font(.body.monospaced())
                        }
                        .accessibilityIdentifier("candidate-alias")
                    }
                } footer: {
                    Text(replace
                        ? "The AI sees the alias; mdoc puts the original back when you restore its reply."
                        : "The original stays in the text you share.")
                }
                if mentionCount > 1 {
                    Section {
                        Picker("Apply to", selection: $scope) {
                            Text("All \(mentionCount) occurrences").tag(Scope.allOccurrences)
                            Text("Just this one").tag(Scope.justThisOne)
                        }
                        .pickerStyle(.inline)
                        .labelsHidden()
                        .accessibilityIdentifier("candidate-scope")
                    } header: {
                        Text("Apply to")
                    }
                }
                if hasNext(scope) {
                    Section {
                        Button("Next proposal") {
                            onDone(replace ? .replace : .keep, scope)
                            onNext()
                        }
                        .accessibilityHint(Text("Applies this decision and opens the next proposal."))
                    }
                }
            }
            // The value is in Found; the title names what it is.
            .navigationTitle(candidate.category.label)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        onDone(replace ? .replace : .keep, scope)
                        dismiss()
                    }
                    .accessibilityIdentifier("candidate-done")
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}

/// Replace… for a selection: picks the category; every exact repeat is
/// replaced with it.
struct ManualReplaceSheet: View {
    let text: String
    let onReplace: (Category) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var category: Category = .person

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(text)
                        .font(.body)
                } header: {
                    Text("Selected text")
                }
                Section {
                    Picker("Category", selection: $category) {
                        ForEach(Category.allCases, id: \.self) { category in
                            Label(category.label, systemImage: category.symbol).tag(category)
                        }
                    }
                } footer: {
                    Text("Every exact repeat in this document is replaced too.")
                }
            }
            // Titled by what the selection becomes, like the candidate
            // sheet; Replace is the button.
            .navigationTitle(category.label)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Replace") {
                        onReplace(category)
                        dismiss()
                    }
                }
            }
        }
        .presentationDetents([.medium])
    }
}
