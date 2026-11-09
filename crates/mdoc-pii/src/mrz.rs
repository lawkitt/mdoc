//! A machine-readable zone's name line (ICAO 9303; issue #495): its
//! surname follows the document's head with no separator
//! ("P<RUSKRENC<<LIDIIA"), yet is a word of its own. So is its document
//! number, glued to its check digit ("7605318428RUS…") or, on a TD1 card,
//! to the head too ("I<NORD231458907<<<").
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
        let states = states();
        Regex::new(&format!(
            r"\A{OPENING}[PIAC](?:[A-Z]|{FILLER})?(?:{states})\z"
        ))
        .expect("the zone's head")
    })
}

/// [`STATES`] as alternatives, each filler as the source may write it.
fn states() -> String {
    STATES
        .iter()
        .map(|state| state.replace('<', FILLER))
        .collect::<Vec<_>>()
        .join("|")
}

/// What opens a TD1 card's lines: an indent and blockquote marks.
const INDENT: &str = r"[\p{Zs}\t]*(?:>[\p{Zs}\t]*)*";

/// What may open a passport's or a TD2 card's second line before its
/// document number (the name line's openings).
fn opening() -> &'static Regex {
    static OPENS: OnceLock<Regex> = OnceLock::new();
    OPENS.get_or_init(|| Regex::new(&format!(r"\A{OPENING}\z")).expect("the zone's opening"))
}

/// A passport's or a TD2 card's second line after its document number:
/// the fillers that pad it, its check digit, the nationality, the birth
/// date and its check digit, the sex, the expiry date and its check digit,
/// then capitals, digits and fillers, and what closes the line.
fn number_line() -> &'static Regex {
    static LINE: OnceLock<Regex> = OnceLock::new();
    LINE.get_or_init(|| {
        let states = states();
        Regex::new(&format!(
            r"\A{FILLER}*(?:[0-9]|{FILLER})(?:{states})[0-9]{{6}}(?:[0-9]|{FILLER})(?:[MFX]|{FILLER})[0-9]{{6}}(?:[0-9]|{FILLER})(?:[A-Z0-9]|{FILLER})*{CLOSING}\z"
        ))
        .expect("the zone's number line")
    })
}

/// A TD1 card's first line before its document number: its indent, the
/// card's code, its type and the issuing state.
fn card_head() -> &'static Regex {
    static HEAD: OnceLock<Regex> = OnceLock::new();
    HEAD.get_or_init(|| {
        let states = states();
        Regex::new(&format!(
            r"\A{INDENT}[IAC](?:[A-Z]|{FILLER})?(?:{states})\z"
        ))
        .expect("the card's head")
    })
}

/// A TD1 card's text after its document number: the fillers that pad it,
/// its check digit and the optional data to the line's end, then (a blank
/// line between too) the card's second line's birth date and its check
/// digit, sex, expiry date and its check digit, and nationality.
fn card_rest() -> &'static Regex {
    static REST: OnceLock<Regex> = OnceLock::new();
    REST.get_or_init(|| {
        Regex::new(&format!(
            r"\A{FILLER}*(?:[0-9]|{FILLER})(?:[A-Z0-9]|{FILLER})*[\p{{Zs}}\t]*\\?\r?\n(?:[\p{{Zs}}\t]*\r?\n)?{INDENT}[0-9]{{6}}(?:[0-9]|{FILLER})(?:[MFX]|{FILLER})[0-9]{{6}}(?:[0-9]|{FILLER})(?:[A-Z]|{FILLER}){{3}}"
        ))
        .expect("the card's lines")
    })
}

/// Whether `range` of `text` stands where a zone's document number does
/// (#495): opening a passport's or a TD2 card's second line, the rest of
/// that line of its shape, or after a TD1 card's head, the card's second
/// line under it. What `range` holds is not read (a damaged number, or its
/// alias), nor is any check digit checked.
pub(crate) fn number_at(text: &str, range: &Range<usize>) -> bool {
    if range.is_empty() || text[range.clone()].contains('\n') {
        return false;
    }
    let start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[start..range.start];
    let after = &text[range.end..];
    let line = &after[..after.find('\n').unwrap_or(after.len())];
    (opening().is_match(before) && number_line().is_match(line))
        || (card_head().is_match(before) && card_rest().is_match(after))
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
