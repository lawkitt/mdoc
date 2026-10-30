import Foundation
#if canImport(UIKit)
import UIKit
#endif

/// Fictional fixture data for `FakeCore`. People, companies, addresses and
/// numbers are invented; the phone number is in Ofcom's drama range, the
/// email uses the reserved example.com domain and the IBAN is the standard
/// documentation example.
enum FakeFixtures {
    struct Entity {
        var category: Category
        /// Longest first; the first is the primary wording.
        var variants: [String]
        var alias: String
    }

    /// The names the fake detector "finds", with the aliases the seeded case
    /// already gave them.
    static let entities: [Entity] = [
        Entity(category: .person, variants: ["Maya Ellison", "Ms Ellison"], alias: "PERSON_1"),
        Entity(category: .person, variants: ["Daniel Okafor", "Mr Okafor"], alias: "PERSON_2"),
        Entity(category: .person, variants: ["Priya Nandakumar"], alias: "PERSON_3"),
        Entity(category: .person, variants: ["Tom Haverley"], alias: "PERSON_4"),
        Entity(category: .organization, variants: ["Brightwater Joinery Ltd", "Brightwater"], alias: "ORG_1"),
        Entity(category: .organization, variants: ["Harrow & Vale Solicitors LLP"], alias: "ORG_2"),
        Entity(category: .address, variants: ["22 Orchard Row, Exeter EX4 9ZZ"], alias: "ADDRESS_1"),
        Entity(category: .address, variants: ["3 Mill Lane, Taunton TA1 9ZZ"], alias: "ADDRESS_2"),
        Entity(category: .address, variants: ["14 Quayside Walk, Exeter EX1 9ZZ"], alias: "ADDRESS_3"),
        Entity(category: .email, variants: ["maya.ellison@example.com"], alias: "EMAIL_1"),
        Entity(category: .phone, variants: ["01632 960 418"], alias: "PHONE_1"),
        Entity(category: .bank, variants: ["GB82 WEST 1234 5698 7654 32"], alias: "BANK_1"),
        Entity(category: .person, variants: ["Grace Whitlow"], alias: "PERSON_5"),
        Entity(category: .organization, variants: ["Harbour Street Lettings"], alias: "ORG_3"),
        Entity(category: .address, variants: ["Flat 2, 9 Harbour Street, Plymouth PL1 9ZZ"], alias: "ADDRESS_4"),
    ]

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
        # Tenancy agreement

        **Landlord:** Harbour Street Lettings
        **Tenant:** Grace Whitlow
        **Property:** Flat 2, 9 Harbour Street, Plymouth PL1 9ZZ
        """,
        """
        ## Rent

        The rent is £1,150 a month, paid on the first day of each month.
        """,
        """
        ## Signatures

        Signed by Grace Whitlow and for Harbour Street Lettings.
        """,
    ]

    static let notePrefix = "Note from a client"

    /// The fake's import warning for PDFs, like the core's partial results.
    static let importWarning = "Page 2 has no text layer and was skipped."

    /// Text "recognized" from a file the user imported.
    static func importedText(title: String) -> String {
        """
        # \(title)

        Ms Ellison called about the staircase again. Daniel Okafor has not \
        replied. Her number is 01632 960 418.
        """
    }

    #if canImport(UIKit)
    /// Writes the fixture's original pages so the Original view has
    /// something to show: a PDF of the letter and page images of the tenancy.
    static func writeOriginals(into folder: URL) throws -> (letter: URL, tenancy: [URL]) {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let letterURL = folder.appendingPathComponent("letter-before-claim.pdf")
        if !FileManager.default.fileExists(atPath: letterURL.path) {
            let renderer = UIGraphicsPDFRenderer(bounds: CGRect(x: 0, y: 0, width: 595, height: 842))
            try renderer.writePDF(to: letterURL) { context in
                context.beginPage()
                drawPage(letter, in: CGRect(x: 56, y: 56, width: 483, height: 730))
            }
        }
        var tenancy: [URL] = []
        for (index, page) in tenancyPages.enumerated() {
            let url = folder.appendingPathComponent("tenancy-\(index + 1).png")
            if !FileManager.default.fileExists(atPath: url.path) {
                let format = UIGraphicsImageRendererFormat()
                format.scale = 2
                let renderer = UIGraphicsImageRenderer(size: CGSize(width: 420, height: 594), format: format)
                let image = renderer.image { context in
                    UIColor(white: 0.97, alpha: 1).setFill()
                    context.fill(CGRect(x: 0, y: 0, width: 420, height: 594))
                    drawPage(page, in: CGRect(x: 32, y: 40, width: 356, height: 520))
                }
                try image.pngData()?.write(to: url)
            }
            tenancy.append(url)
        }
        return (letterURL, tenancy)
    }

    private static func drawPage(_ markdown: String, in rect: CGRect) {
        let plain = markdown
            .replacingOccurrences(of: "**", with: "")
            .replacingOccurrences(of: "# ", with: "")
            .replacingOccurrences(of: "#", with: "")
        let style = NSMutableParagraphStyle()
        style.paragraphSpacing = 6
        let attributes: [NSAttributedString.Key: Any] = [
            .font: UIFont(name: "Georgia", size: 11) ?? UIFont.systemFont(ofSize: 11),
            .foregroundColor: UIColor.black,
            .paragraphStyle: style,
        ]
        NSString(string: plain).draw(in: rect, withAttributes: attributes)
    }
    #endif
}
