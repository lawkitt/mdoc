import SwiftUI

/// Notes (#1091 Decision 3): what the user keeps about the document for
/// themselves, saved with it once typing pauses and when the sheet closes.
/// Copy for AI and Share never send them, and search never reads them.
struct NotesSheet: View {
    let model: DocumentViewModel
    @Environment(\.dismiss) private var dismiss
    @State private var notes = ""
    /// The notes were read in: typing changes them from then on.
    @State private var loaded = false
    @FocusState private var focused: Bool

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: Space.s) {
                Label("Private to this device. Never sent with Copy for AI or Share.", systemImage: "lock")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("notes-hint")
                TextEditor(text: $notes)
                    .focused($focused)
                    .scrollContentBackground(.hidden)
                    .frame(minHeight: 160)
                    .padding(Space.xs)
                    .background(.background.secondary, in: RoundedRectangle(cornerRadius: Radius.control))
                    .overlay(alignment: .topLeading) {
                        if notes.isEmpty {
                            Text("Notes about this document: deadlines, who to call, what to check. Only you see them.")
                                .foregroundStyle(.tertiary)
                                .padding(.top, Space.xs + 8)
                                .padding(.horizontal, Space.xs + 5)
                                .allowsHitTesting(false)
                                .accessibilityHidden(true)
                        }
                    }
                    .accessibilityLabel(Text("Notes"))
                    .accessibilityIdentifier("notes-editor")
            }
            .padding()
            .navigationTitle("Notes")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                        .accessibilityIdentifier("notes-done")
                }
            }
        }
        .onAppear {
            notes = model.notes
            loaded = true
            focused = notes.isEmpty
        }
        .onChange(of: notes) { _, notes in
            if loaded { model.setNotes(notes) }
        }
        .onDisappear { model.saveNotes() }
    }
}
