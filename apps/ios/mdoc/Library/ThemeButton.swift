import SwiftUI

/// The sidebar's theme button, beside Settings (#1070): a tap flips Light
/// and Dark, to the opposite of what shows; a long press picks Automatic,
/// Light or Dark. Its symbol is the theme chosen. Settings › Appearance ›
/// Theme stays.
struct ThemeButton: View {
    @Environment(\.preferences) private var preferences
    @Environment(\.colorScheme) private var colorScheme
    /// As a big circle button's icon (`BigButton`), growing with the text.
    @ScaledMetric private var width = BigButton.circleLabelWidth

    var body: some View {
        let theme = preferences.theme
        Menu {
            Picker(selection: Bindable(preferences).theme) {
                ForEach(AppTheme.allCases) { choice in
                    Label(choice.title, systemImage: choice.symbol).tag(choice)
                }
            } label: {
                Text("Theme")
            }
        } label: {
            Image(systemName: theme.symbol)
                .contentTransition(.symbolEffect(.replace))
                .frame(width: width, height: width + 2 * (BigButton.systemCirclePadding - BigButton.systemPadding))
                .bigButtonLabel(prominent: false)
        } primaryAction: {
            preferences.theme = colorScheme == .dark ? .light : .dark
        }
        .secondaryGlass()
        .buttonBorderShape(.circle)
        .accessibilityLabel(Text("Theme"))
        .accessibilityValue(Text(theme.title))
        .accessibilityHint(Text("Switches between light and dark. Touch and hold for Automatic."))
        .accessibilityIdentifier("theme-button")
    }
}
