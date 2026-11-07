use super::{from_html, pasted_markdown};
use crate::{spans, tables};

fn md(html: &str) -> String {
    from_html(html).expect("text")
}

/// A real copy from the ChatGPT iOS app (2026-11-07), a review of mdoc's
/// fictional sample: the reason this module exists.
const CHATGPT: &str = include_str!("../fixtures/from_html/chatgpt-ios.html");
const CHATGPT_MD: &str = include_str!("../fixtures/from_html/chatgpt-ios.md");

#[test]
fn the_chatgpt_ios_copy_comes_back_as_markdown() {
    // A Windows checkout may give the file CRLF.
    let expected = CHATGPT_MD.replace("\r\n", "\n");
    assert_eq!(md(CHATGPT), expected.trim_end());
}

#[test]
fn the_chatgpt_ios_table_is_a_table_to_the_reader() {
    let markdown = md(CHATGPT);
    let lines: Vec<&str> = markdown.lines().collect();
    let header = lines
        .iter()
        .position(|l| l.starts_with("| Issue"))
        .expect("header");
    assert!(
        tables::is_table_separator(lines[header + 1]),
        "{}",
        lines[header + 1]
    );
    assert_eq!(lines[header], "| Issue | Risk | Explanation |");
    assert!(lines.contains(&"### 1. Parties and legal structure"));
    assert!(lines.contains(&"- Client: ORG_3, at ADDRESS_1."));
}

#[test]
fn every_alias_survives_unescaped() {
    for alias in [
        "ORG_3",
        "ORG_4",
        "ADDRESS_1",
        "ADDRESS_2",
        "PERSON_4",
        "PERSON_5",
    ] {
        assert_eq!(
            md(CHATGPT).matches(alias).count(),
            CHATGPT.matches(alias).count(),
            "{alias}"
        );
    }
    assert_eq!(
        md("<p>Paid by <b>PERSON_1</b> to ORG_2_a.</p>"),
        "Paid by **PERSON_1** to ORG_2_a."
    );
}

#[test]
fn inline_marks() {
    assert_eq!(
        md("<p><b>bold</b> <i>it</i> <del>gone</del> <code>a*b</code></p>"),
        "**bold** *it* ~~gone~~ `a*b`"
    );
    assert_eq!(md("<p>a<b> b </b>c</p>"), "a **b** c");
    assert_eq!(md("<p><b></b>x</p>"), "x");
    assert_eq!(md("<p><b><strong>x</strong></b></p>"), "**x**");
    assert_eq!(md("<p>a<br>b</p>"), "a  \nb");
}

#[test]
fn links_keep_only_safe_targets() {
    assert_eq!(
        md(r#"<p><a href="https://example.com/a b">site</a></p>"#),
        "[site](https://example.com/a%20b)"
    );
    assert_eq!(
        md(r#"<p><a href="javascript:alert(1)">site</a></p>"#),
        "site"
    );
    assert_eq!(md("<p><a>site</a></p>"), "site");
}

#[test]
fn lists_nest() {
    assert_eq!(
        md("<ul><li>a<ul><li>b</li></ul></li><li><p>c</p><p>d</p></li></ul><ol><li>e<li>f</ol>"),
        "- a\n  - b\n- c\n\n  d\n\n1. e\n2. f"
    );
}

#[test]
fn tables_are_rectangular_and_cells_stay_on_one_line() {
    assert_eq!(
        md(
            "<table><tr><th><p><b>A</b></p></th><th>B</th></tr><tr><td><p>1</p><p>2</p></td></tr><tr><td>x|y</td><td><b>z</b></td></tr></table>"
        ),
        "| A | B |\n| --- | --- |\n| 1 2 |  |\n| x\\|y | **z** |"
    );
    assert_eq!(
        md("<table><tr><td>a<td>b<tr><td>c<td>d</table>"),
        "| a | b |\n| --- | --- |\n| c | d |"
    );
}

#[test]
fn blocks() {
    assert_eq!(
        md("<h2><span><b>T</b></span></h2><p>x</p><hr><blockquote><p>q</p><p>r</p></blockquote>"),
        "## T\n\nx\n\n---\n\n> q\n>\n> r"
    );
    assert_eq!(
        md("<pre><code>let a = 1;\n  *b\n</code></pre>"),
        "```\nlet a = 1;\n  *b\n```"
    );
    assert_eq!(md("<p>one<p>two"), "one\n\ntwo");
}

#[test]
fn literal_text_stays_literal() {
    assert_eq!(
        md("<p>2 * 3 and a &lt;tag&gt; &amp;amp; [x]</p>"),
        "2 \\* 3 and a \\<tag> \\&amp; \\[x\\]"
    );
    assert_eq!(md("<p>1. Not a list</p>"), "1\\. Not a list");
    assert_eq!(md("<p># Not a heading</p>"), "\\# Not a heading");
    assert_eq!(md("<p>- Not an item</p>"), "\\- Not an item");
    assert_eq!(md("<p>A &amp; B</p>"), "A & B");
    assert_eq!(md("<p>a&nbsp;b</p>"), "a b");
}

#[test]
fn nothing_is_lost_to_unknown_or_broken_markup() {
    assert_eq!(md("<custom>kept</custom>"), "kept");
    assert_eq!(md("plain <b>open"), "plain **open**");
    assert_eq!(md("</p></b>after"), "after");
    assert_eq!(
        md("<table>caption text<tr><td>a</td></tr></table>"),
        "caption text\n\n| a |\n| --- |"
    );
    assert_eq!(md("<td>stray</td>"), "stray");
    assert_eq!(md("<li>stray</li>"), "stray");
}

#[test]
fn what_is_no_text_goes() {
    assert_eq!(from_html(""), None);
    assert_eq!(
        from_html("<html><head><title>t</title><style>p{}</style></head><body> \n</body></html>"),
        None
    );
    assert_eq!(md("<script>alert(1)</script><p>x</p>"), "x");
}

#[test]
fn deep_nesting_does_not_overflow() {
    let html = "<div><ul><li>".repeat(5000);
    assert!(from_html(&(html + "x")).is_some());
}

#[test]
fn a_structured_copy_pastes_as_its_html() {
    let plain = "Findings\n\n* Client: ORG_3\n\nIssue\tRisk\nFees\tHigh\n";
    let html = "<h3>Findings</h3><ul><li>Client: ORG_3</li></ul><table><tr><th>Issue</th><th>Risk</th></tr><tr><td>Fees</td><td>High</td></tr></table>";
    assert_eq!(
        pasted_markdown(html, Some(plain)).as_deref(),
        Some("### Findings\n\n- Client: ORG_3\n\n| Issue | Risk |\n| --- | --- |\n| Fees | High |")
    );
    assert!(
        pasted_markdown(CHATGPT, None)
            .unwrap()
            .contains("| Issue | Risk | Explanation |")
    );
}

#[test]
fn plain_text_that_is_already_markdown_stays_as_it_is() {
    let html = "<h3>Title</h3><p>Rendered.</p>";
    for plain in [
        "### Title\n\nRendered.",
        "| a |\n| --- |\n| b |",
        "Some **bold** words",
        "1. one\n\n## two",
    ] {
        assert_eq!(pasted_markdown(html, Some(plain)).as_deref(), Some(plain));
    }
}

#[test]
fn html_without_structure_leaves_the_plain_text() {
    // Markdown source in a plain editor: `<p>| a |</p>` isn't a rendered table.
    let html = "<p><span>| a |</span></p><p>| --- |</p>";
    assert_eq!(
        pasted_markdown(html, Some("| a |\n| --- |")).as_deref(),
        Some("| a |\n| --- |")
    );
    assert_eq!(pasted_markdown("<p>x</p>", Some("x")).as_deref(), Some("x"));
    assert_eq!(
        pasted_markdown("<p>x</p>", None).as_deref(),
        Some("x"),
        "no plain text: the HTML's text"
    );
}

#[test]
fn nothing_to_read_gives_the_plain_text_or_nothing() {
    assert_eq!(pasted_markdown("<p></p>", Some("x")).as_deref(), Some("x"));
    assert_eq!(pasted_markdown("<p></p>", None), None);
}

#[test]
fn marks_never_split_an_alias() {
    assert_eq!(md("<p><b>ORG</b>_3 pays.</p>"), "ORG_3 pays.");
    assert_eq!(md("<p>ORG_<b>3</b> pays.</p>"), "ORG_3 pays.");
    assert_eq!(md("<p>O<i>RG_</i>3 pays.</p>"), "ORG_3 pays.");
    assert_eq!(md("<p><b>ORG</b><i>_3</i> pays.</p>"), "ORG_3 pays.");
    assert_eq!(
        md(r#"<p><a href="https://a.example">ORG</a>_3</p>"#),
        "ORG_3"
    );
    assert_eq!(md("<p><code>ORG</code>_3</p>"), "ORG_3");
    assert_eq!(
        md("<table><tr><td><b>PERSON</b>_12</td></tr></table>"),
        "| PERSON_12 |\n| --- |"
    );
    // A whole alias keeps its mark, and marks beside a space stay.
    assert_eq!(
        md("<p>By <b>ORG_3</b> and <i>PERSON_1</i>.</p>"),
        "By **ORG_3** and *PERSON_1*."
    );
    assert_eq!(md("<p><b>a</b> b</p>"), "**a** b");
}

#[test]
fn a_lone_double_star_is_no_markdown() {
    let html = "<table><tr><th>Power</th></tr><tr><td>x**2</td></tr></table>";
    assert_eq!(
        pasted_markdown(html, Some("Power\nx**2")).as_deref(),
        Some("| Power |\n| --- |\n| x\\*\\*2 |")
    );
    assert!(
        pasted_markdown(html, Some("2**3 and 4**5"))
            .unwrap()
            .starts_with("| Power")
    );
    assert_eq!(
        pasted_markdown(html, Some("Some __bold__ here")).as_deref(),
        Some("Some __bold__ here")
    );
    assert_eq!(
        pasted_markdown(html, Some("**Bold** first")).as_deref(),
        Some("**Bold** first")
    );
    assert!(pasted_markdown(html, Some("a __init__.py b")).is_some());
}

/// The reader's own spans: what is literal in the HTML stays unmarked.
#[test]
fn literal_delimiters_stay_literal_to_the_reader() {
    for html in [
        "<p>Keep ~~this~~ literally.</p>",
        "<p>Keep ==this== literally.</p>",
        "<p>Keep __this__ literally.</p>",
        "<p>Keep _this_ literally.</p>",
    ] {
        let markdown = md(html);
        let styled = spans::scan(&markdown);
        assert!(
            styled.iter().all(|span| !span.style.bold
                && !span.style.italic
                && !span.style.strike
                && span.style.bg.is_none()),
            "{html} gave {markdown}"
        );
    }
    assert_eq!(md("<p>ORG_3 and ORG_4_a</p>"), "ORG_3 and ORG_4_a");
}

#[test]
fn code_with_backticks_at_its_edges_keeps_them() {
    assert_eq!(md("<p>Use <code>&#96;x&#96;</code>.</p>"), "Use `` `x` ``.");
    assert_eq!(md("<p><code>a`b</code></p>"), "``a`b``");
    assert_eq!(md("<p><code>plain</code></p>"), "`plain`");
}

#[test]
fn nested_marks_never_split_an_alias() {
    assert_eq!(md("<p><b><i>ORG</i></b>_3</p>"), "ORG_3");
    assert_eq!(md("<p>ORG_<b><i>3</i></b></p>"), "ORG_3");
    assert_eq!(
        md("<p><b><i><del>ORG</del></i></b>_3 pays</p>"),
        "ORG_3 pays"
    );
    assert_eq!(md("<p>By <b><i>ORG_3</i></b>.</p>"), "By ***ORG_3***.");
}

#[test]
fn custom_aliases_stay_whole() {
    for alias in ["CLIENT__1", "A-B_C", "CASE_-_1", "X---Y__Z"] {
        assert_eq!(
            md(&format!("<h2>T</h2><p>Paid {alias} today</p>")),
            format!("## T\n\nPaid {alias} today")
        );
        assert_eq!(
            md(&format!("<p><b>{alias}</b></p>")),
            format!("**{alias}**")
        );
        assert_eq!(
            md(&format!("<table><tr><td>{alias}</td></tr></table>")),
            format!("| {alias} |\n| --- |")
        );
    }
    assert_eq!(md("<p><b>CLIENT</b>-1</p>"), "CLIENT-1");
}

#[test]
fn bold_next_to_letters_is_markdown() {
    let html = "<p>这是<b>重点</b>内容</p>";
    assert_eq!(
        pasted_markdown(html, Some("这是**重点**内容")).as_deref(),
        Some("这是**重点**内容")
    );
    let table = "<table><tr><td>x</td></tr></table>";
    assert!(
        pasted_markdown(table, Some("x**2 + y**2"))
            .unwrap()
            .starts_with('|')
    );
}

/// Every word survives however the markup nests.
#[test]
fn no_text_is_lost_inside_any_markup() {
    for (open, close) in [
        (r#"<a href="https://example.com">"#, "</a>"),
        ("<b>", "</b>"),
        ("<i>", "</i>"),
        ("<code>", "</code>"),
        ("<h2>", "</h2>"),
        ("<pre>", "</pre>"),
        ("<blockquote>", "</blockquote>"),
        ("<ul><li>", "</li></ul>"),
        ("<table><tr><td>", "</td></tr></table>"),
        ("<div>", "</div>"),
    ] {
        for body in [
            "<p>ORG_3 owes</p>",
            "<ul><li>ORG_3 owes</li></ul>",
            "ORG_3 owes",
            "<div><p>ORG_3 owes</p></div>",
        ] {
            let html = format!("<p>before</p>{open}{body}{close}<p>after</p>");
            let markdown = md(&html);
            for word in ["before", "ORG_3", "owes", "after"] {
                assert!(markdown.contains(word), "{html} lost {word}: {markdown}");
            }
        }
    }
}
