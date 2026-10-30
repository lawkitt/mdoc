import SwiftUI

/// The sheet a tapped candidate opens: its alias, Keep or Replace, and All
/// occurrences or Just this one.
struct CandidateSheet: View {
    let candidate: Candidate
    let identity: Identity?
    let onDone: (Decision, Scope) -> Void
    let onNext: () -> Void
    @Environment(\.dismiss) private var dismiss

    @State private var replace: Bool
    @State private var alias: String
    @State private var scope: Scope = .allOccurrences

    init(candidate: Candidate, identity: Identity?, onDone: @escaping (Decision, Scope) -> Void, onNext: @escaping () -> Void) {
        self.candidate = candidate
        self.identity = identity
        self.onDone = onDone
        self.onNext = onNext
        _replace = State(initialValue: candidate.state != .kept)
        _alias = State(initialValue: candidate.alias)
    }

    private var mentionCount: Int { identity?.mentionCount ?? 1 }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    LabeledContent("Found") {
                        Text(candidate.original)
                    }
                    LabeledContent("Category") {
                        Label(candidate.category.label, systemImage: candidate.category.symbol)
                            .foregroundStyle(candidate.category.color)
                    }
                }
                Section {
                    Picker("Decision", selection: $replace) {
                        Text("Replace").tag(true)
                        Text("Keep").tag(false)
                    }
                    .pickerStyle(.segmented)
                    .accessibilityIdentifier("candidate-decision")
                    if replace {
                        TextField("Alias", text: $alias)
                            .font(.body.monospaced())
                            .textInputAutocapitalization(.characters)
                            .autocorrectionDisabled()
                            .accessibilityLabel(Text("Alias"))
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
                Section {
                    Button("Next proposal") {
                        onDone(replace ? .replace(alias: alias) : .keep, scope)
                        onNext()
                    }
                    .accessibilityHint(Text("Applies this decision and opens the next proposal."))
                }
            }
            .navigationTitle(candidate.original)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        onDone(replace ? .replace(alias: alias) : .keep, scope)
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
            .navigationTitle("Replace")
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
