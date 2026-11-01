import SwiftUI
import UIKit

struct SettingsView: View {
    @Bindable var setup: SetupViewModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    // Each model downloads on its own; Download all takes
                    // every missing one (ADR 0090).
                    ForEach(setup.snapshot.offers) { offer in
                        VStack(alignment: .leading, spacing: 8) {
                            ModelRow(offer: offer, state: setup.state(of: offer))
                            if !setup.isWorking && offer.state != .installed {
                                ModelDownloadButton(setup: setup, offer: offer)
                            }
                        }
                    }
                    if setup.waitsForWiFi {
                        Button("Use cellular") {
                            setup.askCellular()
                        }
                        .accessibilityHint(Text("Asks before downloading over cellular."))
                        .accessibilityIdentifier("setup-use-cellular")
                    }
                    if setup.isWorking {
                        Button("Cancel download", role: .cancel) {
                            setup.cancel()
                        }
                    } else if setup.snapshot.missing.count > 1 {
                        Button {
                            setup.download()
                        } label: {
                            Text("Download all (\(setup.snapshot.missingBytes.formatted(.byteCount(style: .file))))")
                        }
                        .accessibilityIdentifier("settings-download-all")
                    }
                } header: {
                    Text("Models")
                } footer: {
                    VStack(alignment: .leading, spacing: 8) {
                        // Below the floor Settings says what the document
                        // says where Find names would be (ADR 0088).
                        if setup.snapshot.tier == .belowFloor {
                            Text(BelowFloor.note)
                                .accessibilityIdentifier("below-floor-note")
                        }
                        Text("Models download once, on Wi-Fi unless you allow cellular, and then run on this device.")
                    }
                }
                Section {
                    LabeledContent("Memory", value: Int64(setup.physicalMemory).formatted(.byteCount(style: .memory)))
                } footer: {
                    Text(Self.storageNote)
                }
                Section {
                    NavigationLink("About mdoc") {
                        AboutView()
                    }
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .cellularQuestion(model: setup)
        }
        .onAppear { setup.refresh() }
    }

    /// A new phone starts empty, and Settings says so (ADR 0085).
    static var storageNote: String {
        if UIDevice.current.userInterfaceIdiom == .pad {
            String(localized: "mdoc's documents stay on this iPad and are not in backups.")
        } else {
            String(localized: "mdoc's documents stay on this iPhone and are not in backups.")
        }
    }
}

/// One model's download in Settings. Borderless, so only the button, not
/// the whole row, starts it.
private struct ModelDownloadButton: View {
    let setup: SetupViewModel
    let offer: ModelOffer

    var body: some View {
        let size = offer.byteSize.formatted(.byteCount(style: .file))
        Button {
            setup.download([offer])
        } label: {
            Label("Download \(size)", systemImage: "arrow.down.circle")
        }
        .buttonStyle(.borderless)
        .accessibilityLabel(Text("Download \(ModelRow.plainName(offer.purpose)), \(size)"))
        .accessibilityIdentifier("settings-download-\(offer.id)")
    }
}

/// The licence notice (GPL with the app store permission, ADR 0086) and the
/// source link.
struct AboutView: View {
    static let sourceURL = URL(string: "https://github.com/lawkitt/mdoc")!
    static let permissionURL = URL(string: "https://github.com/lawkitt/mdoc/blob/main/LICENSE-APP-STORES.md")!

    private var version: String {
        let info = Bundle.main.infoDictionary
        let short = info?["CFBundleShortVersionString"] as? String ?? "?"
        let build = info?["CFBundleVersion"] as? String ?? "?"
        return "\(short) (\(build))"
    }

    var body: some View {
        Form {
            Section {
                LabeledContent("Version", value: version)
                Link(destination: Self.sourceURL) {
                    Label("Source code", systemImage: "chevron.left.forwardslash.chevron.right")
                }
            } footer: {
                Text("mdoc is free, has no accounts and collects no data. Nothing you open leaves this device unless you share it.")
            }
            Section {
                Text("mdoc is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. It is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.")
                    .font(.footnote)
                Text("Additional permission under GNU GPL version 3 section 7: mdoc may be distributed through app stores whose terms add usage restrictions, provided the corresponding source stays available under the GPL.")
                    .font(.footnote)
                Link("Permission text (LICENSE-APP-STORES.md)", destination: Self.permissionURL)
                    .font(.footnote)
            } header: {
                Text("Licence")
            }
        }
        .navigationTitle("About mdoc")
        .navigationBarTitleDisplayMode(.inline)
    }
}
