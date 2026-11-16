import SwiftUI

/// How long a scan takes, in words (ADR 0120): the estimate before a long
/// scan, the time left while it runs, and Settings' speed per 10 pages.
enum ScanTime {
    /// "About 3 minutes on this phone", before a scan the core expects to
    /// take more than about a minute.
    static func estimate(minutes: Int) -> String {
        if UIDevice.current.userInterfaceIdiom == .pad {
            String(localized: "About \(minutes) min on this iPad")
        } else {
            String(localized: "About \(minutes) min on this iPhone")
        }
    }

    /// While a scan runs: "About 3 min left", or under a minute.
    static func left(seconds: UInt64) -> String {
        seconds < 60
            ? String(localized: "Less than a minute left")
            : String(localized: "About \(Int((Double(seconds) / 60).rounded())) min left")
    }

    /// Settings: "About 2 min per 10 pages", or seconds under 90.
    static func perTenPages(seconds: UInt64) -> String {
        seconds < 90
            ? String(localized: "About \(Int(seconds)) s per 10 pages")
            : String(localized: "About \(Int((Double(seconds) / 60).rounded())) min per 10 pages")
    }
}

/// Before a long scan, under Find names: how long it will take here.
struct ScanEstimateNote: View {
    let minutes: Int

    var body: some View {
        Label(ScanTime.estimate(minutes: minutes), systemImage: "clock")
            .font(.footnote)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("scan-estimate")
    }
}
