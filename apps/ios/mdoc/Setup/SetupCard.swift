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
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

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
                ButtonRow(forceColumn: dynamicTypeSize.isAccessibilitySize) {
                    if model.waitsForWiFi {
                        UseCellularButton(model: model)
                    }
                    Button("Cancel", role: .cancel) {
                        model.cancel()
                    }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("setup-cancel")
                }
                .lineLimit(1)
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
        case .belowFloor: String(localized: "Set up text recognition")
        }
    }

    /// Below the floor the card shows only in a document that needs text
    /// recognition; it offers that model alone (ADR 0088).
    private var message: String {
        switch model.snapshot.tier {
        case .detector:
            String(localized: "mdoc downloads its name detector and text recognition once (\(size)). After that, documents are read and checked on this device, offline. Uses Wi-Fi; on cellular, mdoc asks first.")
        case .belowFloor:
            String(localized: "mdoc downloads text recognition once (\(size)). After that, scans and photos are read on this device, offline. Uses Wi-Fi; on cellular, mdoc asks first.")
        }
    }
}

/// What a device below the detector floor says where Find names would be,
/// and in Settings (ADR 0088). The floor is 8 GB (`DETECTOR_FLOOR_BYTES`
/// sits between the 6 GB and 8 GB classes): iPhone 15 Pro and later, and
/// every iPad with an M-series chip. "8 GB" keeps to one line.
@MainActor
enum BelowFloor {
    static var note: String {
        if UIDevice.current.userInterfaceIdiom == .pad {
            String(localized: "This iPad can't find names: the name detector needs 8\u{00A0}GB of memory (an iPad with M1 or later). Select text to mark a name.")
        } else {
            String(localized: "This iPhone can't find names: the name detector needs 8\u{00A0}GB of memory (iPhone 15 Pro or later). Select text to mark a name.")
        }
    }
}

/// One model's name, size and download state.
struct ModelRow: View {
    let offer: ModelOffer
    let state: ModelState
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        // At accessibility sizes the size goes under the names, which then
        // have the row's width (beside it they broke a word a line).
        let stacked = dynamicTypeSize.isAccessibilitySize
        let header = stacked
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 2))
            : AnyLayout(HStackLayout(alignment: .firstTextBaseline))
        VStack(alignment: .leading, spacing: 4) {
            header {
                VStack(alignment: .leading, spacing: 2) {
                    Text(Self.plainName(offer.purpose))
                    // The model's own name, for anyone who wants it.
                    Text(offer.name)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if !stacked {
                    Spacer()
                }
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
            case .waitingForWiFi:
                Label("Waiting for Wi-Fi", systemImage: "wifi.exclamationmark")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
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

/// Use cellular, while a download waits for Wi-Fi: it asks the cellular
/// question (ADR 0088).
struct UseCellularButton: View {
    let model: SetupViewModel

    var body: some View {
        Button {
            model.askCellular()
        } label: {
            Label("Use cellular", systemImage: "antenna.radiowaves.left.and.right")
        }
        .buttonStyle(.bordered)
        .accessibilityHint(Text("Asks before downloading over cellular."))
        .accessibilityIdentifier("setup-use-cellular")
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
            Button("Use cellular") {
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
