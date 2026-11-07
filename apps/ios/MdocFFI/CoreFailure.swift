import Foundation

/// The core's errors, as the app sees them. UniFFI generates `CoreError`
/// and `ReviewError` as internal types, so this module, which compiles the
/// bindings, maps them once, one case for each of their variants
/// (ADR 0104: each shows its own message).
public enum CoreFailure: Error, Equatable, Sendable {
    // MARK: `CoreError`

    /// Reading or converting the file.
    case importFailed(String)
    /// A PDF has pages without a text layer; import again with OCR or with
    /// native text only. Pages are 1-based.
    case needsOcr(pages: [UInt32], pageCount: UInt32)
    /// Text recognition failed.
    case ocr(String)
    /// Finding names failed.
    case detect(String)
    /// The user cancelled the job, or a newer job replaced it.
    case cancelled
    /// A review transition did not apply: the text or mention changed.
    case review(String)
    /// Reading or writing a case, or a case rule (one case per document).
    case caseFailed(String)
    /// Verifying or installing a model file.
    case setup(String)
    /// An argument the core does not accept.
    case invalid(String)
    /// Reading or writing a file.
    case io(String)

    // MARK: `ReviewError`

    /// Copy for AI and Share wait until every proposal is replaced or kept.
    case pendingProposals(count: UInt64)
    /// Rename to an alias another identity has: Same as is the way to it.
    case aliasTaken(alias: String, identity: UInt64)
    /// The mention or identity named is not in the review now.
    case nameNotFound
    /// The core refused the change, in its own words.
    case refused(String)
    /// Reading or writing the document or its case failed.
    case storage(String)

    /// Anything else, in its own words.
    case other(String)

    public init(_ error: any Error) {
        switch error {
        case let error as CoreError:
            self = switch error {
            case .Import(let message): .importFailed(message)
            case .NeedsOcr(let pages, let pageCount): .needsOcr(pages: pages, pageCount: pageCount)
            case .Ocr(let message): .ocr(message)
            case .Detect(let message): .detect(message)
            case .Cancelled: .cancelled
            case .Review(let message): .review(message)
            case .Case(let message): .caseFailed(message)
            case .Setup(let message): .setup(message)
            case .Invalid(let message): .invalid(message)
            case .Io(let message): .io(message)
            }
        case let error as ReviewError:
            self = switch error {
            case .PendingProposals(let count): .pendingProposals(count: count)
            case .AliasTaken(let alias, let identity): .aliasTaken(alias: alias, identity: identity)
            case .NotFound: .nameNotFound
            case .Refused(let message): .refused(message)
            case .Storage(let message): .storage(message)
            }
        case let error as CoreFailure:
            self = error
        default:
            self = .other(error.localizedDescription)
        }
    }
}
