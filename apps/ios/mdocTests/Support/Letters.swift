import Foundation
import UIKit
@testable import mdoc

/// The tests' fictional documents: a letter before claim and a quotation in
/// one case, a tenancy scan in another. People, companies, addresses and
/// numbers are invented; the phone number is in Ofcom's drama range, the
/// email uses the reserved example.com domain and the IBAN is the standard
/// documentation example.
enum Letters {
    /// The names the tests propose (`DocumentSession.propose`), as Try a
    /// sample does: what a scan with the name detector would find.
    static let names: [SampleDocument.Name] = [
        .init(wording: "Maya Ellison", category: .person),
        .init(wording: "Ms Ellison", category: .person),
        .init(wording: "Daniel Okafor", category: .person),
        .init(wording: "Mr Okafor", category: .person),
        .init(wording: "Priya Nandakumar", category: .person),
        .init(wording: "Tom Haverley", category: .person),
        .init(wording: "Brightwater Joinery Ltd", category: .organization),
        .init(wording: "Harrow & Vale Solicitors LLP", category: .organization),
        .init(wording: "22 Orchard Row, Exeter EX4 9ZZ", category: .address),
        .init(wording: "3 Mill Lane, Taunton TA1 9ZZ", category: .address),
        .init(wording: "14 Quayside Walk, Exeter EX1 9ZZ", category: .address),
        .init(wording: "maya.ellison@example.com", category: .email),
        .init(wording: "01632 960 418", category: .phone),
        .init(wording: "GB82 WEST 1234 5698 7654 32", category: .bank),
    ]

    static let ellisonCase = "Ellison v Brightwater"
    static let harbourCase = "Harbour Street lease"

    static let letterTitle = "Letter before claim"

    static let letter = """
        # Letter before claim

        **From:** Harrow & Vale Solicitors LLP, 14 Quayside Walk, Exeter EX1 9ZZ
        **To:** Mr Okafor, Brightwater Joinery Ltd, 3 Mill Lane, Taunton TA1 9ZZ
        **Date:** 2 October 2026

        Dear Mr Okafor,

        ## Our client: Maya Ellison

        We act for Maya Ellison of 22 Orchard Row, Exeter EX4 9ZZ. On 14 June 2026 \
        Brightwater Joinery Ltd fitted a staircase at her home under a written \
        quotation. The staircase has since moved away from the wall and is *unsafe*.

        Ms Ellison reported the defect by email on 3 July 2026 and again to your \
        colleague Priya Nandakumar on 18 July 2026. Neither message was answered.

        ## What our client asks

        1. Repair or replace the staircase within 28 days, **or**
        2. Refund the price of £4,850 to the account below.

        | Account name | IBAN |
        | --- | --- |
        | Maya Ellison | GB82 WEST 1234 5698 7654 32 |

        Please reply to maya.ellison@example.com or call 01632 960 418. If we \
        hear nothing within 14 days, Ms Ellison will issue a claim without \
        further notice.

        Yours sincerely,

        Tom Haverley
        Harrow & Vale Solicitors LLP

        *Fictional letter for demonstration.*
        """

    static let quotationTitle = "Quotation 2026-118"

    static let quotation = """
        # Quotation 2026-118

        **Customer:** Maya Ellison, 22 Orchard Row, Exeter EX4 9ZZ
        **Supplier:** Brightwater Joinery Ltd, 3 Mill Lane, Taunton TA1 9ZZ

        | Item | Price |
        | --- | --- |
        | Oak staircase, fitted | £4,850 |

        Prepared by Daniel Okafor.
        """

    static let tenancyTitle = "Tenancy agreement"

    static let tenancyPages = [
        """
        Tenancy agreement

        Landlord: Harbour Street Lettings
        Tenant: Grace Whitlow
        Property: Flat 2, 9 Harbour Street, Plymouth PL1 9ZZ
        """,
        """
        Rent

        The rent is £1,150 a month, paid on the first day of each month.
        """,
        """
        Signatures

        Signed by Grace Whitlow and for Harbour Street Lettings.
        """,
    ]

    /// Camera pages of `pages`, as JPEGs in `folder`.
    static func pageImages(_ pages: [String], named name: String, in folder: URL) throws -> [URL] {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return try pages.enumerated().map { index, page in
            let url = folder.appendingPathComponent("\(name)-\(index + 1).jpg")
            let format = UIGraphicsImageRendererFormat()
            format.scale = 2
            let renderer = UIGraphicsImageRenderer(size: CGSize(width: 420, height: 594), format: format)
            let data = renderer.jpegData(withCompressionQuality: 0.9) { context in
                UIColor(white: 0.97, alpha: 1).setFill()
                context.fill(CGRect(x: 0, y: 0, width: 420, height: 594))
                draw(page, in: CGRect(x: 32, y: 40, width: 356, height: 520))
            }
            try data.write(to: url)
            return url
        }
    }

    /// A PDF with a text layer, one page per string; an empty string makes
    /// a page with no text.
    static func textPDF(_ pages: [String], named name: String, in folder: URL) throws -> URL {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appendingPathComponent("\(name).pdf")
        let renderer = UIGraphicsPDFRenderer(bounds: CGRect(x: 0, y: 0, width: 595, height: 842))
        try renderer.writePDF(to: url) { context in
            for page in pages {
                context.beginPage()
                if !page.isEmpty {
                    draw(page, in: CGRect(x: 56, y: 56, width: 483, height: 730))
                }
            }
        }
        return url
    }

    private static func draw(_ text: String, in rect: CGRect) {
        let style = NSMutableParagraphStyle()
        style.paragraphSpacing = 6
        let attributes: [NSAttributedString.Key: Any] = [
            .font: UIFont(name: "Georgia", size: 11) ?? UIFont.systemFont(ofSize: 11),
            .foregroundColor: UIColor.black,
            .paragraphStyle: style,
        ]
        NSString(string: text).draw(in: rect, withAttributes: attributes)
    }
}
