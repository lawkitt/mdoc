//! A machine-readable zone's name line (ICAO 9303; issue #495): its
//! surname follows the document's head with no separator
//! ("P<RUSKRENC<<LIDIIA"), yet is a word of its own.
use regex::Regex;
use std::{ops::Range, sync::OnceLock};

/// The issuing states and nationalities a zone may name (ICAO 9303 Part 3):
/// the ISO 3166-1 alpha-3 codes, Germany's `D<<`, the British national
/// types, Kosovo, the EU, the United Nations and its agencies, the other
/// organizations' `X…` codes, and the stateless and unspecified `XX…`. The
/// rule pack's span patterns list the same codes (a test checks it).
pub const STATES: &[&str] = &[
    "ABW", "AFG", "AGO", "AIA", "ALA", "ALB", "AND", "ARE", "ARG", "ARM", "ASM", "ATA", "ATF",
    "ATG", "AUS", "AUT", "AZE", "BDI", "BEL", "BEN", "BES", "BFA", "BGD", "BGR", "BHR", "BHS",
    "BIH", "BLM", "BLR", "BLZ", "BMU", "BOL", "BRA", "BRB", "BRN", "BTN", "BVT", "BWA", "CAF",
    "CAN", "CCK", "CHE", "CHL", "CHN", "CIV", "CMR", "COD", "COG", "COK", "COL", "COM", "CPV",
    "CRI", "CUB", "CUW", "CXR", "CYM", "CYP", "CZE", "DEU", "DJI", "DMA", "DNK", "DOM", "DZA",
    "ECU", "EGY", "ERI", "ESH", "ESP", "EST", "ETH", "FIN", "FJI", "FLK", "FRA", "FRO", "FSM",
    "GAB", "GBR", "GEO", "GGY", "GHA", "GIB", "GIN", "GLP", "GMB", "GNB", "GNQ", "GRC", "GRD",
    "GRL", "GTM", "GUF", "GUM", "GUY", "HKG", "HMD", "HND", "HRV", "HTI", "HUN", "IDN", "IMN",
    "IND", "IOT", "IRL", "IRN", "IRQ", "ISL", "ISR", "ITA", "JAM", "JEY", "JOR", "JPN", "KAZ",
    "KEN", "KGZ", "KHM", "KIR", "KNA", "KOR", "KWT", "LAO", "LBN", "LBR", "LBY", "LCA", "LIE",
    "LKA", "LSO", "LTU", "LUX", "LVA", "MAC", "MAF", "MAR", "MCO", "MDA", "MDG", "MDV", "MEX",
    "MHL", "MKD", "MLI", "MLT", "MMR", "MNE", "MNG", "MNP", "MOZ", "MRT", "MSR", "MTQ", "MUS",
    "MWI", "MYS", "MYT", "NAM", "NCL", "NER", "NFK", "NGA", "NIC", "NIU", "NLD", "NOR", "NPL",
    "NRU", "NZL", "OMN", "PAK", "PAN", "PCN", "PER", "PHL", "PLW", "PNG", "POL", "PRI", "PRK",
    "PRT", "PRY", "PSE", "PYF", "QAT", "REU", "ROU", "RUS", "RWA", "SAU", "SDN", "SEN", "SGP",
    "SGS", "SHN", "SJM", "SLB", "SLE", "SLV", "SMR", "SOM", "SPM", "SRB", "SSD", "STP", "SUR",
    "SVK", "SVN", "SWE", "SWZ", "SXM", "SYC", "SYR", "TCA", "TCD", "TGO", "THA", "TJK", "TKL",
    "TKM", "TLS", "TON", "TTO", "TUN", "TUR", "TUV", "TWN", "TZA", "UGA", "UKR", "UMI", "URY",
    "USA", "UZB", "VAT", "VCT", "VEN", "VGB", "VIR", "VNM", "VUT", "WLF", "WSM", "YEM", "ZAF",
    "ZMB", "ZWE", // ISO 3166-1 alpha-3
    "D<<", "GBD", "GBN", "GBO", "GBP", "GBS", "RKS", "EUE", "UNO", "UNA", "UNK", "XBA", "XIM",
    "XCC", "XCE", "XCO", "XEC", "XPO", "XES", "XMP", "XOM", "XDC", "XXA", "XXB", "XXC", "XXX",
];

/// A filler as the source may write it: `<`, Markdown's escaped `\<` or
/// the entity `&lt;`.
const FILLER: &str = r"(?:<|\\<|&lt;)";

/// What may open the line before the head: an indent (a no-break space
/// too), blockquote marks, a list marker, table cells before the zone's,
/// a label ("MRZ: ") and an emphasis or code mark.
const OPENING: &str = r"[\p{Zs}\t]*(?:>[\p{Zs}\t]*)*(?:(?:[-*+]|\d{1,9}[.)])[\p{Zs}\t]+)?(?:\|(?:[^|\n]*\|)*[\p{Zs}\t]*)?(?:[^\n:<|`*]{1,40}:[\p{Zs}\t]+)?(?:\*{1,2}|_{1,2}|`{1,2})?";

/// What may close the line after the zone: an emphasis or code mark, the
/// table's other cells, a hard break's backslash.
const CLOSING: &str =
    r"(?:\*{1,2}|_{1,2}|`{1,2})?(?:[\p{Zs}\t]*\|[^\n]*)?[\p{Zs}\t]*\\?[\p{Zs}\t\r]*";

/// The line before a surname, from its start: what opens it, then the
/// document's code (`P`, or `I`, `A`, `C` for an ID card), its type (a
/// capital or a filler, which OCR may lose) and the issuing state.
fn head() -> &'static Regex {
    static HEAD: OnceLock<Regex> = OnceLock::new();
    HEAD.get_or_init(|| {
        let states = STATES
            .iter()
            .map(|state| state.replace('<', FILLER))
            .collect::<Vec<_>>()
            .join("|");
        Regex::new(&format!(
            r"\A{OPENING}[PIAC](?:[A-Z]|{FILLER})?(?:{states})\z"
        ))
        .expect("the zone's head")
    })
}

/// The line from a surname on: capitals and fillers alone, then what closes
/// it.
fn rest() -> &'static Regex {
    static REST: OnceLock<Regex> = OnceLock::new();
    REST.get_or_init(|| {
        Regex::new(&format!(r"\A(?:[A-Z]|{FILLER})+{CLOSING}\z")).expect("the zone's line")
    })
}

/// Whether `at` in `text` follows a zone's head on its line.
pub(crate) fn head_before(text: &str, at: usize) -> bool {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    head().is_match(&text[start..at])
}

/// Whether a filler starts at `at` in `text`.
pub(crate) fn filler_at(text: &str, at: usize) -> bool {
    let rest = &text[at..];
    rest.starts_with('<') || rest.starts_with("\\<") || rest.starts_with("&lt;")
}

/// Whether `range` of `text` is a zone's surname right after its head: a
/// filler after it, and the rest of its line capitals and fillers with a
/// double filler among them ("P<RUSKRENC<<LIDIIA<<<").
pub(crate) fn surname_at(text: &str, range: &Range<usize>) -> bool {
    if !filler_at(text, range.end) || !head_before(text, range.start) {
        return false;
    }
    let end = text[range.start..]
        .find('\n')
        .map_or(text.len(), |i| range.start + i);
    let line = &text[range.start..end];
    rest().is_match(line) && line.replace("\\<", "<").replace("&lt;", "<").contains("<<")
}
