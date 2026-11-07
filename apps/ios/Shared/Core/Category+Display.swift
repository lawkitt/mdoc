import Foundation
import MdocFFI

// The core's `Category` as the app names it; its colours and symbols are in
// `Palette`.

extension Category {
    var label: String {
        switch self {
        case .person: String(localized: "Person")
        case .organization: String(localized: "Organization")
        case .email: String(localized: "Email")
        case .url: String(localized: "Web address")
        case .phone: String(localized: "Phone")
        case .address: String(localized: "Address")
        case .date: String(localized: "Date")
        case .caseNumber: String(localized: "Case number")
        case .identity: String(localized: "Identity")
        case .tax: String(localized: "Tax identifier")
        case .bank: String(localized: "Bank details")
        case .other: String(localized: "Other")
        }
    }
}
