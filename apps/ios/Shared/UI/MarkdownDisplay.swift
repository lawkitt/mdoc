import Foundation

/// How Markdown text is shown: rendered as desktop draws it, or as source
/// with dimmed markers while Show Markdown is on (ADR 0096). One setting for
/// the app and its share extension, so it lives in the App Group.
enum MarkdownDisplay {
    static let showsSourceKey = "showsMarkdownSource"
    static var store: UserDefaults { UserDefaults(suiteName: MdocStorage.appGroup) ?? .standard }
}
