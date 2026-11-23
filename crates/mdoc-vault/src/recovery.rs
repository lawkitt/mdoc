//! The recovery key: the Vault key as text a person can write down
//! (#1091 Decision 22). RFC 4648 base32 without padding: 52 characters
//! for 32 bytes, shown in 13 groups of 4. Reading one forgives its form
//! (case, spaces, hyphens, and 0, 1 and 8 for O, I and B), never its
//! content: there is no checksum, so a mistyped key is another key, and
//! [`crate::Vault::open`] answers [`crate::VaultError::WrongKey`].

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
/// Characters in a recovery key: ⌈256 / 5⌉.
pub const LEN: usize = 52;
/// Characters in a group as shown.
pub const GROUP: usize = 4;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryKeyError {
    /// A character that is not in a recovery key.
    #[error("A recovery key has only the letters A–Z and the digits 2–7; it has “{0}”.")]
    Character(char),
    /// Not 52 characters once spaces and hyphens are left out.
    #[error("A recovery key has {LEN} characters; this has {0}.")]
    Length(usize),
    /// The last character's spare bits are set: it was mistyped.
    #[error("The last character of this recovery key is mistyped.")]
    Trailing,
}

/// `key` as its recovery key, in groups of [`GROUP`] separated by spaces.
pub fn format(key: &[u8; 32]) -> String {
    let mut text = String::with_capacity(LEN + LEN / GROUP);
    let (mut buffer, mut bits, mut count) = (0u32, 0u32, 0usize);
    let mut push = |index: u32, text: &mut String| {
        if count > 0 && count % GROUP == 0 {
            text.push(' ');
        }
        text.push(ALPHABET[index as usize] as char);
        count += 1;
    };
    for &byte in key {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            push((buffer >> bits) & 31, &mut text);
        }
    }
    if bits > 0 {
        push((buffer << (5 - bits)) & 31, &mut text);
    }
    text
}

/// The key a typed or pasted recovery key stands for.
pub fn parse(text: &str) -> Result<[u8; 32], RecoveryKeyError> {
    let mut values = Vec::with_capacity(LEN);
    for c in text.chars() {
        if c.is_whitespace() || matches!(c, '-' | '‐' | '‑' | '–' | '—') {
            continue;
        }
        let c = match c.to_ascii_uppercase() {
            '0' => 'O',
            '1' => 'I',
            '8' => 'B',
            c => c,
        };
        let value = ALPHABET
            .iter()
            .position(|&a| a as char == c)
            .ok_or(RecoveryKeyError::Character(c))?;
        values.push(value as u32);
    }
    if values.len() != LEN {
        return Err(RecoveryKeyError::Length(values.len()));
    }
    let mut key = [0u8; 32];
    let (mut buffer, mut bits, mut at) = (0u32, 0u32, 0usize);
    for value in values {
        buffer = ((buffer << 5) | value) & 0xfff;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            key[at] = (buffer >> bits) as u8;
            at += 1;
        }
    }
    // 52 × 5 = 260 bits: the 4 left over must be zero.
    if buffer & ((1 << bits) - 1) != 0 {
        return Err(RecoveryKeyError::Trailing);
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_keys_read_as_rfc_4648() {
        // RFC 4648's own vector, widened: 32 zero bytes are 52 A's.
        assert_eq!(format(&[0; 32]).replace(' ', ""), "A".repeat(52));
        let mut key = [0u8; 32];
        key[..5].copy_from_slice(b"fooba");
        // RFC 4648 §10: BASE32("fooba") = "MZXW6YTB".
        assert!(format(&key).replace(' ', "").starts_with("MZXW6YTB"));
        // 32 0xff bytes end in one set bit and four spare zero bits: Q.
        let all = format(&[0xff; 32]).replace(' ', "");
        assert_eq!(all, format!("{}Q", "7".repeat(51)));
    }
}
