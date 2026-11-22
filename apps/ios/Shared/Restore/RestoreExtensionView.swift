import SwiftUI
import UIKit

/// The Restore with mdoc sheet: the reply restored with one case, a case
/// picker, then Copy, Share and Save to document.
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
            if model.cases.isEmpty {
                ContentUnavailableView(
                    "No cases yet",
                    systemImage: "folder",
                    description: Text(CoreError.noCase.errorDescription ?? ""))
            } else {
                casePicker(model)
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

    private func casePicker(_ model: RestoreViewModel) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Menu {
                Picker("Case", selection: Binding(
                    get: { model.selectedCase },
                    set: { if let id = $0 { model.selectCase(id) } })
                ) {
                    ForEach(model.cases) { item in
                        Label(item.name, systemImage: "folder").tag(Optional(item.id))
                    }
                }
            } label: {
                HStack {
                    Text("Case")
                        .foregroundStyle(.secondary)
                    Text(model.selectedCaseName ?? "")
                        .fontWeight(.semibold)
                    Image(systemName: "chevron.up.chevron.down")
                        .font(.caption)
                        .accessibilityHidden(true)
                }
            }
            .accessibilityLabel(Text("Case: \(model.selectedCaseName ?? "")"))
            .accessibilityHint(Text("Restores the reply with another case's names."))
            .accessibilityIdentifier("restore-case-picker")
            if let result = model.result {
                RestoreSummary(result: result)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
    }

    /// One row when the labels fit on one line each, else stacked.
    /// A Save to document that failed says why under them.
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
        Menu {
            ForEach(model.documentsInCase) { document in
                Button {
                    model.save(to: document.id)
                } label: {
                    if document.id == model.defaultSaveTarget?.id {
                        Label(document.title, systemImage: "star")
                    } else {
                        Text(document.title)
                    }
                }
            }
        } label: {
            Label(model.savedTo == nil ? "Save to document" : "Saved", systemImage: model.savedTo == nil ? "tray.and.arrow.down" : "checkmark")
                .frame(maxWidth: .infinity)
                .bigButtonLabel(prominent: false)
        } primaryAction: {
            if let target = model.defaultSaveTarget {
                model.save(to: target.id)
            }
        }
        .secondaryGlass()
        .disabled(model.documentsInCase.isEmpty)
        .accessibilityHint(Text("Keeps the reply with \(model.defaultSaveTarget?.title ?? "")."))
    }
}
