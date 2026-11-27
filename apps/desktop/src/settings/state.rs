//! Which Settings page shows: the categories, in the order the list
//! offers them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Category {
    #[default]
    General,
    Detection,
    Recognition,
    Spelling,
    Vault,
    AiConnector,
    About,
}

impl Category {
    pub const ALL: [Self; 7] = [
        Self::General,
        Self::Detection,
        Self::Recognition,
        Self::Spelling,
        Self::Vault,
        Self::AiConnector,
        Self::About,
    ];
}
