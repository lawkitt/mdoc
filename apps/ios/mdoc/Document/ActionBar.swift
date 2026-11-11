import SwiftUI
import UIKit

/// The primary action, as on desktop: Recognize, Find names, running with
/// Cancel, Replace N, then Share, Copy and Paste (a reply).
struct ActionBar: View {
    @Bindable var model: DocumentViewModel

    var body: some View {
        if model.barActsOnReply {
            replyButtons
        } else {
            documentButtons
        }
    }

    @ViewBuilder
    private var documentButtons: some View {
        switch model.primaryAction {
        case .recognize(let resume):
            primary(resume ? "Resume recognition" : "Recognize text", systemImage: "text.viewfinder") {
                Task { await model.recognize() }
            }
        case .finishImport:
            primary("Finish import", systemImage: "doc.badge.arrow.up") {
                Task { await model.recognize() }
            }
        case .findNames:
            primary("Find names", systemImage: "text.magnifyingglass") {
                Task { await model.findNames() }
            }
        case .running(let kind, let progress):
            HStack(spacing: 12) {
                Group {
                    // The core reports stages; a fraction only when it knows one.
                    if let fraction = progress?.fraction {
                        ProgressView(value: fraction) {
                            Text(Self.progressLabel(kind, progress))
                                .font(.footnote)
                        }
                    } else {
                        HStack(spacing: 8) {
                            ProgressView()
                            Text(Self.progressLabel(kind, progress))
                                .font(.footnote)
                            Spacer(minLength: 0)
                        }
                        .accessibilityElement(children: .combine)
                    }
                }
                .accessibilityIdentifier("job-progress")
                Button("Cancel", role: .cancel) {
                    model.cancelJob()
                }
                .secondaryAction()
            }
        case .waiting:
            Text("Waiting for another document to finish.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity)
        case .replaceAll(let count):
            layout {
                Button {
                    model.beginReview()
                } label: {
                    Label("Review", systemImage: "list.bullet.rectangle")
                        .frame(maxWidth: .infinity)
                }
                .secondaryAction()
                primary("Replace \(count)", systemImage: "eye.slash") {
                    model.replaceAllPending()
                }
                .accessibilityLabel(Text("Replace all \(count) proposals"))
                .accessibilityHint(Text("Each gets its alias. You can still keep any of them."))
            }
        case .handOff:
            VStack(spacing: Space.xs) {
                if !model.snapshot.candidates.isEmpty {
                    AllReviewedLine(candidates: model.snapshot.candidates)
                }
                handOffButtons
            }
        }
    }

    /// On the Reply pane the bar acts on the reply (ADR 0087), also in
    /// place of Review and Replace N where the reply covers the text
    /// (ADR 0111). Short labels keep three buttons in one row on a phone;
    /// VoiceOver says what each acts on.
    private var replyButtons: some View {
        layout {
            Button {
                model.shareReply()
            } label: {
                Label("Share", systemImage: "square.and.arrow.up")
                    .frame(maxWidth: .infinity)
            }
            .primaryAction()
            .accessibilityLabel(Text("Share reply"))
            .accessibilityIdentifier("share-reply")
            Button {
                model.copyReply { RestoredPasteboard.copy($0) }
            } label: {
                Label("Copy", systemImage: "doc.on.doc")
                    .frame(maxWidth: .infinity)
            }
            .secondaryAction()
            .accessibilityLabel(Text("Copy reply"))
            .accessibilityIdentifier("copy-reply")
            pasteReply(accessibilityLabel: "Paste another reply")
        }
    }

    /// Share, Copy and Paste once every name is decided.
    private var handOffButtons: some View {
            layout {
                // One tap shares the text; the file is a long press away.
                Button {
                    model.share(asFile: false)
                } label: {
                    Label("Share", systemImage: "square.and.arrow.up")
                        .frame(maxWidth: .infinity)
                }
                .primaryAction()
                .contextMenu {
                    Button {
                        model.share(asFile: true)
                    } label: {
                        Label("Share as Markdown file", systemImage: "doc")
                    }
                }
                .accessibilityAction(named: Text("Share as Markdown file")) {
                    model.share(asFile: true)
                }
                .accessibilityIdentifier("primary-action")
                Button {
                    model.copyForAI { UIPasteboard.general.string = $0 }
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                        .frame(maxWidth: .infinity)
                }
                .secondaryAction()
                .accessibilityHint(Text("Copies the text for an AI app, with the note about placeholders."))
                .accessibilityIdentifier("copy-for-ai")
                pasteReply(accessibilityLabel: "Paste reply")
            }
    }

    /// Paste, which opens the reply sheet.
    private func pasteReply(accessibilityLabel: LocalizedStringKey) -> some View {
        Button {
            model.showsReplySheet = true
        } label: {
            Label("Paste", systemImage: "arrow.uturn.backward")
                .frame(maxWidth: .infinity)
        }
        .secondaryAction()
        .accessibilityLabel(Text(accessibilityLabel))
        .accessibilityIdentifier("paste-reply")
    }

    /// The buttons side by side when every label fits on one line, stacked
    /// otherwise: a label never wraps inside its button (ADR 0087). Like the
    /// system's own bars, the bar's text stops growing at the second
    /// accessibility size, where the primary button keeps a row and the
    /// others share one, so the text keeps most of the screen; a long press
    /// shows a label in the large content viewer.
    private func layout<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        ButtonRow {
            Group { content() }
                .accessibilityShowsLargeContentViewer()
        }
        .lineLimit(1)
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
    }

    private func primary(_ title: LocalizedStringKey, systemImage: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .frame(maxWidth: .infinity)
        }
        .primaryAction()
        .accessibilityIdentifier("primary-action")
    }

    static func progressLabel(_ kind: JobKind, _ progress: JobUpdate?) -> String {
        if let page = progress?.page, let count = progress?.pageCount, count > 0 {
            return String(localized: "Recognizing page \(min(page + 1, count)) of \(count)")
        }
        switch progress?.stage {
        case .reading: return String(localized: "Reading the file")
        case .converting: return String(localized: "Converting")
        case .recognizing: return String(localized: "Recognizing text")
        case .scanning: return String(localized: "Finding names")
        case nil:
            switch kind {
            case .recognize: return String(localized: "Reading the file")
            case .scan: return String(localized: "Finding names")
            case .install: return String(localized: "Installing")
            }
        }
    }
}

/// Below the detector floor, where Find names would be (ADR 0088).
struct BelowFloorNote: View {
    var body: some View {
        Label(BelowFloor.note, systemImage: "info.circle")
            .font(.footnote)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("below-floor-note")
    }
}
