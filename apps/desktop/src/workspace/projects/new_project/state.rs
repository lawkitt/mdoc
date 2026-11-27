//! New Project's state: the name a taken one becomes. No GPUI.

use crate::workspace::projects::text;
use std::collections::HashSet;

/// `name`, or "name 2", "name 3"… when a Project has it already: the
/// phones' rule (iOS and Android `unusedName`): two Projects of one name
/// could not be told apart in the Project pickers.
pub(in crate::workspace) fn unused_name(name: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(name) {
        return name.to_string();
    }
    (2..)
        .map(|n| text::numbered(name, n))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_name_is_numbered_from_two() {
        let taken = |names: &[&str]| names.iter().map(|n| n.to_string()).collect();
        assert_eq!(unused_name("Smith", &taken(&[])), "Smith");
        assert_eq!(unused_name("Smith", &taken(&["Jones"])), "Smith");
        assert_eq!(unused_name("Smith", &taken(&["Smith"])), "Smith 2");
        assert_eq!(
            unused_name("Smith", &taken(&["Smith", "Smith 2", "Smith 4"])),
            "Smith 3"
        );
        // Exact, as on the phones.
        assert_eq!(unused_name("smith", &taken(&["Smith"])), "smith");
    }
}
