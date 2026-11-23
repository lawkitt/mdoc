import SwiftUI
import UIKit

/// The Restore with mdoc sheet: the reply restored with one place, a
/// picker of Projects and Unfiled documents, then Copy, Share and Save.
struct RestoreExtensionView: View {
    let sheet: RestoreSheet
    let close: () -> Void

    @State private var share: ShareItems?

    var body: some View {
        NavigationStack {
            Group {
                switch sheet {
                case .ready(let model):
                    content(model)
                case .noText:
                    ContentUnavailableView(
                        "Nothing to restore",
                        systemImage: "text.badge.xmark",
                        description: Text("Share the AI's reply as text to restore it."))
                case .failed(let message):
                    notRestored(message)
                }
            }
            .navigationTitle("Restore with mdoc")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done", action: close)
                }
            }
        }
        .sheet(item: $share) { items in
            ActivityView(items: items.items)
        }
        // The app's theme, from the App Group (ADR 0099).
        .preferredColorScheme(Preferences.shared.theme.colorScheme)
    }

    @ViewBuilder
    private func content(_ model: RestoreViewModel) -> some View {
        VStack(spacing: 0) {
            if model.places.isEmpty {
                ContentUnavailableView(
                    "No documents yet",
                    systemImage: "folder",
                    description: Text(CoreError.noProject.errorDescription ?? ""))
            } else {
                placePicker(model)
                Divider()
                if let result = model.result {
                    RestoredTextView(result: result)
                    Divider()
                    actions(model, result: result)
                } else if let message = model.errorMessage {
                    notRestored(message)
                }
            }
        }
    }

    private func notRestored(_ message: String) -> some View {
        ContentUnavailableView("Not restored", systemImage: "exclamationmark.triangle", description: Text(message))
    }

    /// Projects, then Unfiled documents (#1091, the maintainer's (a)).
    private func placePicker(_ model: RestoreViewModel) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Menu {
                Picker("Restore with", selection: Binding(
                    get: { model.selected },
                    set: { if let place = $0 { model.select(place) } })
                ) {
                    if !model.places.projects.isEmpty {
                        Section("Projects") {
                            ForEach(model.places.projects) { item in
                                Label(item.name, systemImage: "folder").tag(Optional(RestorePlace.project(item.id)))
                            }
                        }
                    }
                    if !model.places.unfiled.isEmpty {
                        Section("Unfiled") {
                            ForEach(model.places.unfiled) { item in
                                Label(item.title, systemImage: "doc.text").tag(Optional(RestorePlace.document(item.id)))
                            }
                        }
                    }
                }
            } label: {
                HStack {
                    Text(placeKind(model.selected))
                        .foregroundStyle(.secondary)
                    Text(model.selectedName ?? "")
                        .fontWeight(.semibold)
                    Image(systemName: "chevron.up.chevron.down")
                        .font(.caption)
                        .accessibilityHidden(true)
                }
            }
            .accessibilityLabel(Text("\(placeKind(model.selected)): \(model.selectedName ?? "")"))
            .accessibilityHint(Text("Restores the reply with another Project's or document's names."))
            .accessibilityIdentifier("restore-case-picker")
            if let result = model.result {
                RestoreSummary(result: result)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
    }

    /// What the picker shows the place as: a Project, or an Unfiled document.
    private func placeKind(_ place: RestorePlace?) -> String {
        if case .document = place { String(localized: "Document") } else { String(localized: "Project") }
    }

    /// One row when the labels fit on one line each, else stacked.
    /// A Save that failed says why under them.
    private func actions(_ model: RestoreViewModel, result: RestoreResult) -> some View {
        VStack(spacing: 8) {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 12) { buttons(model, result: result) }
                VStack(spacing: 12) { buttons(model, result: result) }
            }
            .secondaryAction()
            .labelStyle(.titleAndIcon)
            .lineLimit(1)
            if let message = model.errorMessage {
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding()
    }

    @ViewBuilder
    private func buttons(_ model: RestoreViewModel, result: RestoreResult) -> some View {
        // Copy is what most replies need (back into the chat or a document),
        // so it is the one prominent button.
        Button {
            model.copy()
        } label: {
            Label(model.copied ? "Copied" : "Copy", systemImage: model.copied ? "checkmark" : "doc.on.doc")
                .frame(maxWidth: .infinity)
        }
        .primaryAction()
        Button {
            share = ShareItems(items: [result.text])
        } label: {
            Label("Share", systemImage: "square.and.arrow.up")
                .frame(maxWidth: .infinity)
        }
        // Keeps the reply, in alias form, for the app to list restored.
        Button {
            model.save()
        } label: {
            Label(model.saved == nil ? "Save" : "Saved", systemImage: model.saved == nil ? "tray.and.arrow.down" : "checkmark")
                .frame(maxWidth: .infinity)
        }
        .disabled(model.saved != nil)
        .accessibilityHint(Text("Keeps the reply in mdoc with \(model.selectedName ?? "")."))
        .accessibilityIdentifier("restore-save")
    }
}

