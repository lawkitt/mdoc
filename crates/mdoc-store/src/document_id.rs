//! What names a document across the desktop's live views, its session and
//! its lookups: a Vault document by its opaque id, which the session stores
//! instead of a path (ADR 0125). Documents are never plain files.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DocumentId {
    /// A Vault document.
    Vault(VaultId),
}

impl DocumentId {
    /// The Vault document this is.
    pub fn vault(&self) -> &VaultId {
        let Self::Vault(id) = self;
        id
    }
}

/// A Vault object's id: 32 lowercase hex digits, checked wherever one is
/// read, so a session can never name a file outside the Vault.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct VaultId(String);

impl VaultId {
    pub fn parse(id: &str) -> Option<Self> {
        (id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
            .then(|| Self(id.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for VaultId {
    type Error = InvalidVaultId;
    fn try_from(id: String) -> Result<Self, Self::Error> {
        Self::parse(&id).ok_or(InvalidVaultId)
    }
}

impl From<VaultId> for String {
    fn from(id: VaultId) -> Self {
        id.0
    }
}

impl std::fmt::Display for VaultId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid Vault id.")]
pub struct InvalidVaultId;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_lowercase_32_hex_ids_parse() {
        assert!(VaultId::parse(&"0a".repeat(16)).is_some());
        for bad in [
            "a".repeat(31),
            "a".repeat(33),
            "A".repeat(32),
            format!("{}g", "a".repeat(31)),
            format!("../{}", "a".repeat(29)),
            String::new(),
        ] {
            assert!(VaultId::parse(&bad).is_none(), "{bad:?}");
            assert!(serde_json::from_value::<VaultId>(bad.into()).is_err());
        }
    }
}
