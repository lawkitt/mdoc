import Foundation

/// Stepping limited to one identity or one category (ADR 0098's focus),
/// started from Identities.
enum ReviewFocus: Hashable, Sendable {
    case identity(IdentityID)
    case category(Category)

    func contains(_ candidate: Candidate) -> Bool {
        switch self {
        case .identity(let id): candidate.identityID == id
        case .category(let category): candidate.category == category
        }
    }
}

/// Where the review bar stands, in document order. Candidate ids are source
/// ranges, which move when a replacement changes the text, so the bar
/// keeps its place by position in the ordered list (decided mentions stay
/// in it).
struct ReviewOrder: Equatable {
    let candidates: [Candidate]

    init(_ snapshot: DocumentSnapshot, focus: ReviewFocus? = nil) {
        let all = snapshot.candidates.sorted { $0.range.start < $1.range.start }
        candidates = focus.map { focus in all.filter { focus.contains($0) } } ?? all
    }

    var count: Int { candidates.count }
    var decided: Int { candidates.filter { $0.state != .proposed }.count }
    var isDone: Bool { !candidates.isEmpty && candidates.allSatisfy { $0.state != .proposed } }

    func index(of id: CandidateID?) -> Int? {
        guard let id else { return nil }
        return candidates.firstIndex { $0.id == id }
    }

    /// The first undecided candidate after `index`, wrapping round; nil when
    /// none is left.
    func nextUndecided(after index: Int?) -> Candidate? {
        guard !candidates.isEmpty else { return nil }
        let start = (index ?? -1) + 1
        for offset in 0..<candidates.count {
            let candidate = candidates[(start + offset) % candidates.count]
            if candidate.state == .proposed { return candidate }
        }
        return nil
    }

    /// The candidate at the same position, or the nearest one, after a
    /// change moved the ranges.
    func candidate(at index: Int) -> Candidate? {
        guard !candidates.isEmpty else { return nil }
        return candidates[min(max(index, 0), candidates.count - 1)]
    }

    /// The candidate whose range holds `offset`, else the first after it.
    func candidate(near offset: Int) -> Candidate? {
        candidates.first { $0.range.start <= offset && offset < max($0.range.end, $0.range.start + 1) }
            ?? candidates.first { $0.range.start >= offset }
            ?? candidates.last
    }
}

extension Category {
    /// Contacts and identifiers, which can belong to a person or company
    /// (desktop's Belongs to…, ADR 0032).
    var hasOwner: Bool {
        switch self {
        case .email, .url, .phone, .address, .identity, .tax, .bank: true
        case .person, .organization, .date, .caseNumber, .other: false
        }
    }
}
