import Foundation

/// The core's error, as the app sees it. UniFFI generates `CoreError` as an
/// internal type, so this module, which compiles the bindings, maps it once.
public enum CoreFailure: Error, Equatable, Sendable {
    /// A PDF has pages without a text layer; import again with OCR or with
    /// native text only. Pages are 1-based.
    case needsOcr(pages: [UInt32], pageCount: UInt32)
    /// The user cancelled the job, or a newer job replaced it.
    case cancelled
    /// Anything else, in the words of the crate that failed.
    case failed(String)

    public init(_ error: any Error) {
        switch error {
        case let error as CoreError:
            switch error {
            case .NeedsOcr(let pages, let pageCount):
                self = .needsOcr(pages: pages, pageCount: pageCount)
            case .Cancelled:
                self = .cancelled
            case .Import(let message), .Ocr(let message), .Detect(let message), .Review(let message),
                 .Case(let message), .Setup(let message), .Invalid(let message), .Io(let message):
                self = .failed(message)
            }
        case let error as CoreFailure:
            self = error
        default:
            self = .failed(error.localizedDescription)
        }
    }
}
