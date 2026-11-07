import Foundation

// `CaseIterable` for the generated enums whose order the app shows: the
// variants in their Rust declaration order, as the generated converter
// numbers them (1, 2, …), so the order is the core's and no list here can
// fall behind it.

extension Category: CaseIterable {
    /// In `mdoc_pii::Category`'s order, which the category menus list (#682).
    public static let allCases = declaredCases(FfiConverterTypeCategory.read)
}

/// Every variant a fieldless enum's converter reads, from 1 until the first
/// number it does not know. A variant with fields would end the list early;
/// `testCategoriesFollowTheCoresOrder` lists every category, so it would fail.
private func declaredCases<T>(_ read: (inout (data: Data, offset: Data.Index)) throws -> T) -> [T] {
    var cases: [T] = []
    var variant: Int32 = 1
    while true {
        var buffer = (data: withUnsafeBytes(of: variant.bigEndian) { Data($0) }, offset: 0)
        guard let value = try? read(&buffer) else { return cases }
        cases.append(value)
        variant += 1
    }
}
