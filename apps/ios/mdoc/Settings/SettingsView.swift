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
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        let size = offer.byteSize.formatted(.byteCount(style: .file))
        Button {
            setup.download([offer])
        } label: {
            // At accessibility sizes the size goes under the word, with no
            // icon beside it, so the word keeps its line: squeezed, it broke
            // as "Down- / load 18…".
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading) {
                    Text("Download")
                    Text(size)
                }
            } else {
                Label("Download \(size)", systemImage: "arrow.down.circle")
            }
        }
        .buttonStyle(.borderless)
        .accessibilityLabel(Text("Download \(ModelRow.plainName(offer.purpose)), \(size)"))
        .accessibilityIdentifier("settings-download-\(offer.id)")
    }
}

/// The version, build number and core commit of this build, from Info.plist
/// (ADR 0091, 0097). The commit is the source this build was made from.
struct BuildInfo: Equatable {
    static let repositoryURL = URL(string: "https://github.com/lawkitt/mdoc")!

    let version: String
    let build: String
    /// The full commit hash, or nil for a build made without git.
    let commit: String?

    init(info: [String: Any]?) {
        version = info?["CFBundleShortVersionString"] as? String ?? "?"
        build = info?["CFBundleVersion"] as? String ?? "?"
        let raw = info?["MdocCoreCommit"] as? String ?? ""
        let isHash = raw.count >= 7 && raw.allSatisfy { $0.isHexDigit }
        commit = isHash ? raw.lowercased() : nil
    }

    static var current: BuildInfo { BuildInfo(info: Bundle.main.infoDictionary) }

    var versionText: String { "\(version) (\(build))" }
    var shortCommit: String? { commit.map { String($0.prefix(7)) } }

    /// The source tree at this build's commit, or the repository without one.
    var sourceURL: URL {
        guard let commit else { return Self.repositoryURL }
        return Self.repositoryURL.appending(path: "tree/\(commit)")
    }
}

/// The license notice (GPL with the app store permission, ADR 0086) and the
/// source link, which opens the commit this build was made from.
struct AboutView: View {
    static let permissionURL = URL(string: "https://github.com/lawkitt/mdoc/blob/main/LICENSE-APP-STORES.md")!

    private let build = BuildInfo.current

    var body: some View {
        Form {
            Section {
                LabeledContent("Version", value: build.versionText)
                if let commit = build.shortCommit {
                    LabeledContent("Core commit") {
                        Text(commit).monospaced()
                    }
                }
                Link(destination: build.sourceURL) {
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
                Text("License")
            }
        }
        .navigationTitle("About mdoc")
        .navigationBarTitleDisplayMode(.inline)
    }
}
