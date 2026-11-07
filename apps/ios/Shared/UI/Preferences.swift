import Foundation
import Observation
import SwiftUI

/// What the user chose and what the app has learned about them, in one
/// store: the App Group's defaults, which the Restore extension reads too
/// (ADR 0104). Theme, the document font and Show Markdown (Settings ›
/// Appearance), haptics, the review's hints and the setup card. UI tests
/// keep theirs under a prefix of their own, cleared at each launch, so a
/// test never changes the user's choices.
@Observable
final class Preferences: @unchecked Sendable {
    enum Key: String, CaseIterable {
        case theme
        case serif = "documentFontSerif"
        case showsSource = "showsMarkdownSource"
        case haptics
        case swipeLearned = "hint.swipe.learned"
        case longPressLearned = "hint.longPress.learned"
        case grabberBounced = "hint.grabber.bounced"
        case nudged = "reviewNudged"
        case setupCardDismissed
    }

    /// The keys the app kept in its own defaults before they moved to the
    /// App Group; read once into it, so nobody loses a choice.
    static let movedKeys: [Key] = [.haptics, .swipeLearned, .longPressLearned, .grabberBounced, .nudged, .setupCardDismissed]

    @ObservationIgnored private let store: UserDefaults
    @ObservationIgnored private let prefix: String

    /// `store` holds every preference; `testing` keeps them apart from the
    /// user's, under a prefix of their own.
    init(store: UserDefaults, testing: Bool = false) {
        self.store = store
        prefix = testing ? "uiTesting." : ""
    }

    /// The app's and the extension's preferences, in the App Group.
    static let shared: Preferences = {
        let store = UserDefaults(suiteName: MdocStorage.appGroup) ?? .standard
        let preferences = Preferences(store: store, testing: CoreFactory.opensUITestLibrary)
        if !CoreFactory.opensUITestLibrary {
            preferences.moveIn(from: .standard)
        }
        return preferences
    }()

    private func name(_ key: Key) -> String { prefix + key.rawValue }

    private func bool(_ key: Key, default value: Bool = false) -> Bool {
        store.object(forKey: name(key)) as? Bool ?? value
    }

    // MARK: Appearance

    /// Settings › Appearance › Theme (ADR 0099).
    var theme: AppTheme {
        get {
            access(keyPath: \.theme)
            return AppTheme(rawValue: store.string(forKey: name(.theme)) ?? "") ?? .automatic
        }
        set {
            withMutation(keyPath: \.theme) { store.set(newValue.rawValue, forKey: name(.theme)) }
        }
    }

    /// Settings › Appearance › Document font: Serif (New York).
    var serif: Bool {
        get { access(keyPath: \.serif); return bool(.serif) }
        set { withMutation(keyPath: \.serif) { store.set(newValue, forKey: name(.serif)) } }
    }

    /// Show Markdown: the source with dimmed markers (ADR 0096).
    var showsSource: Bool {
        get { access(keyPath: \.showsSource); return bool(.showsSource) }
        set { withMutation(keyPath: \.showsSource) { store.set(newValue, forKey: name(.showsSource)) } }
    }

    // MARK: Review

    /// Settings › Review › Haptics, on until turned off.
    var haptics: Bool {
        get { access(keyPath: \.haptics); return bool(.haptics, default: true) }
        set { withMutation(keyPath: \.haptics) { store.set(newValue, forKey: name(.haptics)) } }
    }

    /// The grabber's one bounce that says the bar pulls up.
    var grabberBounced: Bool {
        get { access(keyPath: \.grabberBounced); return bool(.grabberBounced) }
        set { withMutation(keyPath: \.grabberBounced) { store.set(newValue, forKey: name(.grabberBounced)) } }
    }

    /// The card's one nudge that says it swipes.
    var nudged: Bool {
        get { access(keyPath: \.nudged); return bool(.nudged) }
        set { withMutation(keyPath: \.nudged) { store.set(newValue, forKey: name(.nudged)) } }
    }

    /// A review hint's key (`swipeLearned`, `longPressLearned`) is set: the
    /// hint was used once, or dismissed, and never shows again.
    func isLearned(_ key: Key) -> Bool {
        bool(key)
    }

    func learn(_ key: Key) {
        store.set(true, forKey: name(key))
    }

    /// Settings › Review › Reset tips: every hint shows again, at once.
    func resetTips() {
        for key in [Key.swipeLearned, .longPressLearned] {
            store.removeObject(forKey: name(key))
        }
        withMutation(keyPath: \.grabberBounced) { store.removeObject(forKey: name(.grabberBounced)) }
        withMutation(keyPath: \.nudged) { store.removeObject(forKey: name(.nudged)) }
    }

    // MARK: Setup

    /// Not now on Start's setup card.
    var setupCardDismissed: Bool {
        get { access(keyPath: \.setupCardDismissed); return bool(.setupCardDismissed) }
        set { withMutation(keyPath: \.setupCardDismissed) { store.set(newValue, forKey: name(.setupCardDismissed)) } }
    }

    // MARK: Store

    /// A UI test's launch: every preference back to its default.
    func reset() {
        for key in Key.allCases {
            store.removeObject(forKey: name(key))
        }
    }

    /// Copies the keys the app kept in `old` before they moved here, once;
    /// a value already here wins.
    func moveIn(from old: UserDefaults) {
        guard old !== store else { return }
        for key in Self.movedKeys {
            guard let value = old.object(forKey: key.rawValue) else { continue }
            if store.object(forKey: name(key)) == nil {
                store.set(value, forKey: name(key))
            }
            old.removeObject(forKey: key.rawValue)
        }
    }
}

extension EnvironmentValues {
    /// The preferences views read and change (`Preferences.shared`, or a
    /// test's own).
    @Entry var preferences: Preferences = .shared
}
