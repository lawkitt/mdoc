import SwiftUI
import UIKit

/// Settings (ADR 0099): Appearance, Review, Models, Privacy and About, as
/// inset-grouped rows with icon tiles.
struct SettingsView: View {
    @Bindable var setup: SetupViewModel
    /// About's Try a sample: closes Settings and opens the sample.
    var onTrySample: () -> Void = {}
    @Environment(\.dismiss) private var dismiss
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @AppStorage(AppTheme.key, store: MarkdownDisplay.store) private var theme = AppTheme.automatic.rawValue
    @AppStorage(MarkdownDisplay.serifKey, store: MarkdownDisplay.store) private var serif = false
    @AppStorage(MarkdownDisplay.showsSourceKey, store: MarkdownDisplay.store) private var showsSource = false
    @AppStorage(Haptics.enabledKey) private var haptics = true
    @State private var tipsReset = false

    var body: some View {
        NavigationStack {
            Form {
                appearance
                Section {
                    Toggle(isOn: $haptics) {
                        SettingsLabel("Haptics", systemImage: "hand.tap", color: .pink)
                    }
                    .accessibilityIdentifier("settings-haptics")
                    Button {
                        ReviewTips.resetOnNextLaunch()
                        tipsReset = true
                    } label: {
                        SettingsLabel("Reset tips", systemImage: "lightbulb", color: .yellow)
                    }
                    .foregroundStyle(.primary)
                    .accessibilityIdentifier("settings-reset-tips")
                } header: {
                    Text("Review")
                } footer: {
                    if tipsReset {
                        Text("The tips show again the next time you open mdoc.")
                    }
                }
                models
                Section {
                    NavigationLink {
                        PrivacyView()
                    } label: {
                        SettingsLabel(localized: Self.privacyTitle, systemImage: "lock.fill", color: .green)
                    }
                    .accessibilityIdentifier("settings-privacy")
                }
                Section {
                    NavigationLink {
                        AboutView(onTrySample: onTrySample)
                    } label: {
                        SettingsLabel("About mdoc", systemImage: "info", color: .gray)
                    }
                    .accessibilityIdentifier("settings-about")
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

    private var appearance: some View {
        Section("Appearance") {
            // Segments while they fit; a menu at accessibility sizes.
            if dynamicTypeSize.isAccessibilitySize {
                Picker(selection: $theme) {
                    ForEach(AppTheme.allCases) { Text($0.title).tag($0.rawValue) }
                } label: {
                    SettingsLabel("Theme", systemImage: "circle.lefthalf.filled", color: .indigo)
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier("settings-theme")
            } else {
                VStack(alignment: .leading, spacing: Space.s) {
                    SettingsLabel("Theme", systemImage: "circle.lefthalf.filled", color: .indigo)
                    Picker("Theme", selection: $theme) {
                        ForEach(AppTheme.allCases) { Text($0.title).tag($0.rawValue) }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .accessibilityIdentifier("settings-theme")
                }
                .padding(.vertical, Space.xxs)
            }
            Picker(selection: $serif) {
                Text("System").tag(false)
                Text("Serif").tag(true)
            } label: {
                SettingsLabel("Document font", systemImage: "textformat", color: .orange)
            }
            .pickerStyle(.menu)
            .accessibilityIdentifier("settings-document-font")
            // The source as Copy for AI sends it, markers dimmed (ADR 0096).
            Toggle(isOn: $showsSource) {
                SettingsLabel("Show Markdown", systemImage: "number", color: .blue)
            }
            .accessibilityIdentifier("settings-show-markdown")
        }
    }

    private var models: some View {
        Section {
            // Each model downloads on its own; Download all takes every
            // missing one (ADR 0090). Details hold the model's own name.
            ForEach(setup.snapshot.offers) { offer in
                NavigationLink {
                    ModelDetailView(setup: setup, offerID: offer.id)
                } label: {
                    ModelSummaryRow(offer: offer, state: setup.state(of: offer))
                }
                .accessibilityIdentifier("settings-model-\(offer.id)")
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
            VStack(alignment: .leading, spacing: Space.xs) {
                // Below the floor Settings says what the document
                // says where Find names would be (ADR 0088).
                if setup.snapshot.tier == .belowFloor {
                    Text(BelowFloor.note)
                        .accessibilityIdentifier("below-floor-note")
                }
                Text("Models download once, on Wi-Fi unless you allow cellular, and then run on this device.")
            }
        }
    }

    static var privacyTitle: String {
        if UIDevice.current.userInterfaceIdiom == .pad {
            String(localized: "Documents stay on this iPad")
        } else {
            String(localized: "Documents stay on this iPhone")
        }
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

/// A Settings row's label: a colored icon tile and the title.
struct SettingsLabel: View {
    let title: Text
    let systemImage: String
    let color: Color

    init(_ title: LocalizedStringKey, systemImage: String, color: Color) {
        self.title = Text(title)
        self.systemImage = systemImage
        self.color = color
    }

    /// A title already localized (a model's plain name).
    init(localized title: String, systemImage: String, color: Color) {
        self.title = Text(verbatim: title)
        self.systemImage = systemImage
        self.color = color
    }

    var body: some View {
        Label {
            title
        } icon: {
            IconTile(systemImage: systemImage, color: color)
        }
    }
}

/// A model in Settings' list: its plain name, and its state and size.
struct ModelSummaryRow: View {
    let offer: ModelOffer
    let state: ModelState

    var body: some View {
        LabeledContent {
            Text(Self.summary(offer: offer, state: state))
                .monospacedDigit()
        } label: {
            SettingsLabel(
                localized: ModelRow.plainName(offer.purpose),
                systemImage: offer.purpose == .detector ? "person.text.rectangle" : "text.viewfinder",
                color: offer.purpose == .detector ? .purple : .teal)
        }
    }

    /// "Installed · 632 MB", "Downloading 40%", "632 MB".
    static func summary(offer: ModelOffer, state: ModelState) -> String {
        let size = offer.byteSize.formatted(.byteCount(style: .file))
        switch state {
        case .installed: return String(localized: "Installed · \(size)")
        case .downloading(let fraction): return String(localized: "Downloading \(fraction.formatted(.percent.precision(.fractionLength(0))))")
        case .verifying: return String(localized: "Installing")
        case .waitingForWiFi: return String(localized: "Waiting for Wi-Fi")
        case .failed: return String(localized: "Failed · \(size)")
        case .notInstalled: return size
        }
    }
}

/// One model: its own name, size and state, its download, and for the
/// name finder whether this device has the memory for it (ADR 0088, 0099).
struct ModelDetailView: View {
    @Bindable var setup: SetupViewModel
    let offerID: String

    private var offer: ModelOffer? { setup.snapshot.offers.first { $0.id == offerID } }

    var body: some View {
        Form {
            if let offer {
                Section {
                    ModelRow(offer: offer, state: setup.state(of: offer))
                    if !setup.isWorking && offer.state != .installed {
                        ModelDownloadButton(setup: setup, offer: offer)
                    }
                    if setup.waitsForWiFi {
                        UseCellularButton(model: setup)
                    }
                }
                if offer.purpose == .detector {
                    Section {
                        LabeledContent(Self.deviceName, value: Self.memoryText(setup.physicalMemory))
                    } footer: {
                        Text(setup.snapshot.tier == .detector
                            ? String(localized: "Enough memory to find names on this device.")
                            : BelowFloor.note)
                    }
                }
            }
        }
        .navigationTitle(offer.map { ModelRow.plainName($0.purpose) } ?? "")
        .navigationBarTitleDisplayMode(.inline)
    }

    static var deviceName: String {
        UIDevice.current.userInterfaceIdiom == .pad ? String(localized: "This iPad") : String(localized: "This iPhone")
    }

    /// The memory as phones are sold with it: 11.45 GiB reported is 12 GB.
    static func memoryText(_ bytes: UInt64) -> String {
        let gigabytes = (Double(bytes) / 1_073_741_824).rounded(.up)
        return String(localized: "\(Int(gigabytes)) GB")
    }
}

/// Where documents live, and what leaves the device.
struct PrivacyView: View {
    var body: some View {
        Form {
            Section {
                Label(SettingsView.storageNote, systemImage: "iphone")
                Label("Case keys stay in this device's Keychain.", systemImage: "key")
                Label("Nothing you open leaves this device unless you share it.", systemImage: "square.and.arrow.up")
            } footer: {
                Text("mdoc is free, has no accounts and collects no data.")
            }
        }
        .navigationTitle("Privacy")
        .navigationBarTitleDisplayMode(.inline)
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

    var onTrySample: () -> Void = {}
    private let build = BuildInfo.current

    var body: some View {
        Form {
            Section {
                Button {
                    onTrySample()
                } label: {
                    Label("Try a sample", systemImage: "doc.text.magnifyingglass")
                }
                .accessibilityIdentifier("about-try-sample")
            } footer: {
                Text("A short agreement with made-up names, to try review without your own documents.")
            }
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

#if DEBUG
#Preview("Settings") {
    SettingsView(setup: PreviewFixtures.app().setup)
}

#Preview("Settings, dark, AX5") {
    SettingsView(setup: PreviewFixtures.app().setup)
        .preferredColorScheme(.dark)
        .environment(\.dynamicTypeSize, .accessibility5)
}
#endif
