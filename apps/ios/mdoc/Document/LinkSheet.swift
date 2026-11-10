import SwiftUI

/// Edit mode's link sheet (ADR 0109): the link's text and URL, filled from
/// the link at the caret or the selection. Done makes the link, one edit;
/// for a link being edited an empty URL removes it and keeps its text.
struct LinkSheet: View {
    let draft: LinkDraft
    @State private var text: String
    @State private var url: String
    /// Done ran: it runs once.
    @State private var finished = false
    @FocusState private var focus: Field?
    @Environment(\.dismiss) private var dismiss

    private enum Field { case text, url }

    init(draft: LinkDraft) {
        self.draft = draft
        _text = State(initialValue: draft.text)
        _url = State(initialValue: draft.url)
    }

    /// Done needs a URL, unless an empty one removes the edited link.
    private var canFinish: Bool {
        draft.editing || !url.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Text", text: $text)
                        .focused($focus, equals: .text)
                        .submitLabel(.next)
                        .onSubmit { focus = .url }
                        .accessibilityIdentifier("link-text")
                    TextField("URL", text: $url, prompt: Text(verbatim: "https://"))
                        .keyboardType(.URL)
                        .textContentType(.URL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .focused($focus, equals: .url)
                        .submitLabel(.done)
                        .onSubmit(finish)
                        .accessibilityIdentifier("link-url")
                } footer: {
                    if draft.editing {
                        Text("Clear the URL to remove the link and keep its text.")
                    }
                }
            }
            .navigationTitle(draft.editing ? Text("Edit link") : Text("Add link"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                    .accessibilityIdentifier("link-cancel")
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done", action: finish)
                        .disabled(!canFinish)
                        .accessibilityIdentifier("link-done")
                }
            }
            .onAppear { focus = draft.text.isEmpty ? .text : .url }
            // Done, Cancel or swiped away: the text gets the keyboard back.
            .onDisappear { draft.closed() }
        }
        .presentationDetents([.medium, .large])
    }

    private func finish() {
        guard canFinish, !finished else { return }
        finished = true
        draft.finish(text, url)
        dismiss()
    }
}
