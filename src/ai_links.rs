//! Links that open an AI app with a prompt filled in and not sent (#1164
//! Milestone 2 items 1, 2 and 10). Only the text given is in a link: the
//! caller passes a prompt that names documents by their AI labels alone.
//!
//! - **Claude:** `claude://claude.ai/new?q=…` opens a new chat with the
//!   prompt in its message box (item 1's spike, up to 13,977 characters).
//! - **ChatGPT:** `codex://new?prompt=…`, the desktop app's documented
//!   scheme, opens a new local thread with the prompt in its composer.
//!   Only its Work threads call local tools (item 2's spike).

/// A new Claude chat with `prompt` filled in.
pub(crate) fn claude(prompt: &str) -> String {
    format!("claude://claude.ai/new?q={}", percent_encoded(prompt))
}

/// A new ChatGPT thread with `prompt` filled in.
pub(crate) fn chatgpt(prompt: &str) -> String {
    format!("codex://new?prompt={}", percent_encoded(prompt))
}

/// `text` with every byte but RFC 3986's unreserved ones as `%XX`, so a
/// space is `%20` and a newline `%0A`, as the spike sent them.
pub(crate) fn percent_encoded(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_carry_the_prompt_percent_encoded() {
        let prompt = "Read “Lease (2)” & sign?\nNext: a+b=c/d#e";
        assert_eq!(
            claude(prompt),
            "claude://claude.ai/new?q=Read%20%E2%80%9CLease%20%282%29%E2%80%9D%20%26%20sign%3F\
             %0ANext%3A%20a%2Bb%3Dc%2Fd%23e"
        );
        assert_eq!(
            chatgpt("List my documents in mdoc."),
            "codex://new?prompt=List%20my%20documents%20in%20mdoc."
        );
        assert_eq!(percent_encoded("A-z_0.9~"), "A-z_0.9~");
        assert_eq!(percent_encoded("a+b/ü"), "a%2Bb%2F%C3%BC");
    }
}
