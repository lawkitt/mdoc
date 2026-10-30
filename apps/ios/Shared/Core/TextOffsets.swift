import Foundation

/// Converts the core's UTF-8 source offsets to the UTF-16 offsets UIKit uses,
/// and back. Built once per Markdown text.
struct TextOffsets: Sendable {
    /// UTF-16 offset of each UTF-8 offset, `utf8Count + 1` entries. An offset
    /// inside a multi-byte scalar maps to that scalar's start.
    private let utf16ForUTF8: [Int32]

    init(_ text: String) {
        var table: [Int32] = []
        table.reserveCapacity(text.utf8.count + 1)
        var utf16: Int32 = 0
        for scalar in text.unicodeScalars {
            let bytes = UTF8.width(scalar)
            for _ in 0..<bytes {
                table.append(utf16)
            }
            utf16 += Int32(UTF16.width(scalar))
        }
        table.append(utf16)
        utf16ForUTF8 = table
    }

    var utf8Count: Int { utf16ForUTF8.count - 1 }

    func utf16(_ utf8: Int) -> Int {
        Int(utf16ForUTF8[min(max(utf8, 0), utf8Count)])
    }

    func nsRange(_ range: SourceRange) -> NSRange {
        let start = utf16(range.start)
        return NSRange(location: start, length: utf16(range.end) - start)
    }

    /// The first UTF-8 offset at or after a UTF-16 offset.
    func utf8(_ utf16: Int) -> Int {
        let target = Int32(utf16)
        var low = 0
        var high = utf8Count
        while low < high {
            let mid = (low + high) / 2
            if utf16ForUTF8[mid] < target {
                low = mid + 1
            } else {
                high = mid
            }
        }
        return low
    }

    func sourceRange(_ range: NSRange) -> SourceRange {
        SourceRange(start: utf8(range.location), end: utf8(range.location + range.length))
    }
}
