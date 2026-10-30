import SwiftUI

/// A restored reply with the values put back marked, and alias-shaped tokens
/// no identity owns highlighted as unrecognised.
struct RestoredTextView: View {
    let result: RestoreResult

    var body: some View {
        ScrollView {
            Text(Self.attributed(result))
                .font(.body)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding()
                .accessibilityIdentifier("restored-text")
        }
    }

    static func attributed(_ result: RestoreResult) -> AttributedString {
        var text = AttributedString()
        for segment in result.segments {
            switch segment {
            case .text(let plain):
                text += AttributedString(plain)
            case .restored(let value, _):
                var piece = AttributedString(value)
                piece.backgroundColor = Color.accentColor.opacity(0.15)
                piece.underlineStyle = .single
                text += piece
            case .unresolved(let token):
                var piece = AttributedString(token)
                piece.backgroundColor = Color.orange.opacity(0.3)
                piece.font = .body.monospaced()
                text += piece
            }
        }
        return text
    }
}

/// "7 values restored · 1 unrecognised".
struct RestoreSummary: View {
    let result: RestoreResult

    var body: some View {
        Text(Self.text(result))
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .accessibilityIdentifier("restore-summary")
    }

    static func text(_ result: RestoreResult) -> String {
        let restored = String(localized: "\(result.restoredCount) values restored")
        guard result.unresolvedCount > 0 else { return restored }
        let unresolved = String(localized: "\(result.unresolvedCount) unrecognised")
        return "\(restored) · \(unresolved)"
    }
}
