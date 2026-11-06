import SwiftUI
import TipKit

// One-time tips for the review's gestures (ADR 0098), each shown once;
// Settings › Review › Reset tips shows them again. UI tests turn them off.

struct SwipeTip: Tip {
    var title: Text { Text("Swipe for the next one") }
    var message: Text? { Text("Swipe the card sideways, or tap the edge, to step through the names.") }
    var image: Image? { Image(systemName: "hand.draw") }
}

struct DetailsTip: Tip {
    @Parameter static var stepped: Bool = false

    var title: Text { Text("Pull up for details") }
    var message: Text? { Text("Every mention of this name, and Keep or Replace for all of them.") }
    var image: Image? { Image(systemName: "chevron.up") }
    var rules: [Rule] { [#Rule(Self.$stepped) { $0 }] }
}

struct LongPressTip: Tip {
    var title: Text { Text("Long-press a name") }
    var message: Text? { Text("Replace, keep, say who it is, or change its category without the bar.") }
    var image: Image? { Image(systemName: "hand.tap") }
}

struct UndoTip: Tip {
    @Parameter static var decided: Bool = false

    var title: Text { Text("Changed your mind?") }
    var message: Text? { Text("Undo is beside Keep; you can also shake the phone. A long press on Undo redoes.") }
    var image: Image? { Image(systemName: "arrow.uturn.backward") }
    var rules: [Rule] { [#Rule(Self.$decided) { $0 }] }
}

enum ReviewTips {
    static let resetKey = "resetTipsOnLaunch"

    /// At launch: tips on, reset first when Settings asked for it; off for
    /// UI tests, whose taps a tip would cover.
    static func configure(testing: Bool) {
        if testing {
            Tips.hideAllTipsForTesting()
        }
        if UserDefaults.standard.bool(forKey: resetKey) {
            UserDefaults.standard.removeObject(forKey: resetKey)
            try? Tips.resetDatastore()
        }
        try? Tips.configure([.displayFrequency(.immediate)])
    }

    /// Reset tips: the store can only be reset before it is configured,
    /// so the tips come back the next time mdoc opens.
    static func resetOnNextLaunch() {
        UserDefaults.standard.set(true, forKey: resetKey)
    }
}
