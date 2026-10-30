import SwiftUI
import UIKit

struct SettingsView: View {
    @Bindable var setup: SetupViewModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    ForEach(setup.snapshot.offers) { offer in
                        ModelRow(offer: offer, state: setup.state(of: offer))
                    }
                    if setup.isWorking {
                        Button("Cancel download", role: .cancel) {
                            setup.cancel()
                        }
                    } else if setup.snapshot.needsSetup {
                        Button {
                            setup.download()
                        } label: {
                            Text("Download \(setup.snapshot.missingBytes.formatted(.byteCount(style: .file)))")
                        }
                    }
                } header: {
                    Text("Models")
                } footer: {
                    Text(setup.snapshot.tier == .detector
                        ? "Models download once, on Wi-Fi unless you allow cellular, and then run on this device."
                        : "This device has too little memory for the name detector. Emails, phone numbers, IBANs and other structured details are still found, and you can mark names by hand.")
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
            .cellularQuestion(model: setup, size: setup.snapshot.missingBytes.formatted(.byteCount(style: .file)))
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
