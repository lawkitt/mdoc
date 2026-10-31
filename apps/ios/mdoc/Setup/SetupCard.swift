import SwiftUI
import UIKit

/// The first-use card: what downloads, its size, and that processing then
/// stays on the device (ADR 0026).
struct SetupCard: View {
    @Bindable var model: SetupViewModel
    /// Shows Not now.
    var dismissible = true
    /// What Not now does; on Home it hides the card until next launch.
    var onDismiss: (() -> Void)?
    /// The work that asked for setup, run once the models are installed.
    var then: (@MainActor () async -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(title, systemImage: "arrow.down.circle")
                .font(.headline)
            Text(message)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if model.isWorking {
                ForEach(model.snapshot.offers) { offer in
                    ModelRow(offer: offer, state: model.state(of: offer))
                }
                Button("Cancel", role: .cancel) {
                    model.cancel()
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("setup-cancel")
            } else {
                HStack {
                    Button {
                        model.download(then: then)
                    } label: {
                        if then == nil {
                            Text("Download \(size)")
                        } else {
                            Text("Download \(size) & continue")
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("setup-download")
                    if dismissible {
                        Button("Not now") {
                            if let onDismiss {
                                onDismiss()
                            } else {
                                model.dismissCard()
                            }
                        }
                        .buttonStyle(.borderless)
                    }
                }
            }
            if let error = model.errorMessage {
                Text(error)
                    .font(.footnote)
                    .foregroundStyle(.red)
            }
        }
        .padding()
        .frame(maxWidth: .infinity, alignment: .leading)
        // The grouped row colour: white on Home's grey list in light mode, a
        // lifted grey in dark (the plain secondary background matched the list).
        .background(Color(uiColor: .secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 12))
        .cellularQuestion(model: model, size: size)
    }

    private var size: String {
        model.snapshot.missingBytes.formatted(.byteCount(style: .file))
    }

    private var title: String {
        switch model.snapshot.tier {
        case .detector: String(localized: "Set up name finding")
        case .rulesOnly: String(localized: "Set up text recognition")
        }
    }

    private var message: String {
        switch model.snapshot.tier {
        case .detector:
            String(localized: "mdoc downloads its name detector and text recognition once (\(size)). After that, documents are read and checked on this device, offline. Uses Wi-Fi; on cellular, mdoc asks first.")
        case .rulesOnly where UIDevice.current.userInterfaceIdiom == .pad:
            String(localized: "This iPad has too little memory for the name detector. mdoc still finds email addresses, and phone numbers, IBANs and other details next to their labels; you mark names by selecting them. Text recognition downloads once (\(size)).")
        case .rulesOnly:
            String(localized: "This iPhone has too little memory for the name detector. mdoc still finds email addresses, and phone numbers, IBANs and other details next to their labels; you mark names by selecting them. Text recognition downloads once (\(size)).")
        }
    }
}

/// One model's name, size and download state.
struct ModelRow: View {
    let offer: ModelOffer
    let state: ModelState

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(Self.plainName(offer.purpose))
                    // The model's own name, for anyone who wants it.
                    Text(offer.name)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Text(offer.byteSize.formatted(.byteCount(style: .file)))
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            switch state {
            case .notInstalled:
                Text("Not downloaded")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            case .downloading(let fraction):
                ProgressView(value: fraction) {
                    Text("Downloading")
                        .font(.footnote)
                }
            case .verifying:
                ProgressView {
                    Text("Checking and installing")
                        .font(.footnote)
                }
            case .installed:
                Label("Installed", systemImage: "checkmark.circle")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            case .failed(let message):
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(.red)
            }
        }
        .accessibilityElement(children: .combine)
    }

    /// What the model does, in the reader's words (ADR 0087).
    static func plainName(_ purpose: ModelPurpose) -> String {
        switch purpose {
        case .detector: String(localized: "Name detector")
        case .ocr: String(localized: "Text recognition, English and Russian")
        }
    }
}

extension View {
    /// Asks before a download on cellular; waiting keeps it for Wi-Fi.
    func cellularQuestion(model: SetupViewModel, size: String) -> some View {
        modifier(CellularQuestion(model: model, size: size))
    }
}

private struct CellularQuestion: ViewModifier {
    @Bindable var model: SetupViewModel
    let size: String

    func body(content: Content) -> some View {
        content.confirmationDialog(
            Text("Download \(size) over cellular?"),
            isPresented: $model.asksCellular,
            titleVisibility: .visible
        ) {
            Button("Use Cellular") {
                model.answerCellular(useCellular: true)
            }
            Button("Wait for Wi-Fi") {
                model.answerCellular(useCellular: false)
            }
        } message: {
            Text("Your carrier may charge for the data.")
        }
    }
}
