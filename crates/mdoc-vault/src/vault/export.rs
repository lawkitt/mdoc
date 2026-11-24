//! Export Project… (#1091 Decision 22): a Project's documents written out
//! as plain files, so the Vault is never a trap. Each Document's Markdown
//! (its own text, names and all) and its Source Original, decrypted, go
//! into a new folder named after the Project, inside a folder the user
//! picks outside the Vault:
//!
//! ```text
//! <picked>/<Project name>/
//!   <Title>.md
//!   Originals/<original file name>
//! ```
//!
//! Names are made safe on Windows and macOS and unique, ignoring case;
//! nothing is ever overwritten.
use super::Vault;
use crate::{VaultError, original};
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

/// The Originals subfolder's name.
pub const ORIGINALS: &str = "Originals";
/// Longest name stem written, in bytes, leaving room for " 99" and an
/// extension under the 255-byte limit of common file systems.
const MAX_STEM: usize = 120;

/// What an export wrote.
#[derive(Debug)]
pub struct Exported {
    /// The new folder holding the Project's files.
    pub folder: PathBuf,
    /// Documents written in full.
    pub documents: usize,
    /// Documents that could not be written in full, by title, with why;
    /// what was written of them stays.
    pub failed: Vec<(String, VaultError)>,
}

impl Vault {
    /// Write Project `id`'s documents into a new folder inside `into`,
    /// which must lie outside the Vault. A pasted or new document has no
    /// original; one whose conversion never finished has no Markdown, and
    /// one whose text was cleared since has an empty one.
    pub fn export_project(&self, id: &str, into: &Path) -> Result<Exported, VaultError> {
        let project = self.project(id)?;
        // Checked before anything is created, whatever alias names it.
        if resolved(into)?.starts_with(std::fs::canonicalize(&self.root)?) {
            return Err(VaultError::ExportInsideVault);
        }
        std::fs::create_dir_all(into)?;
        let folder = new_folder(into, &safe_stem(&project.name))?;
        let mut names = Names::default();
        let mut originals = Names::default();
        let mut exported = Exported {
            folder: folder.clone(),
            documents: 0,
            failed: Vec::new(),
        };
        let mut summaries = self.documents_in(Some(id));
        // Oldest first, so a repeated title's " 2" goes to the later one.
        summaries.sort_by(|a, b| (a.created, &a.title, &a.id).cmp(&(b.created, &b.title, &b.id)));
        for summary in summaries {
            let written = self.document(&summary.id).and_then(|document| {
                // Moved out of the Project since the list was read.
                if document.project.as_deref() != Some(id) {
                    return Ok(false);
                }
                // Only a conversion that never finished has no Markdown; a
                // text its user cleared is written, empty.
                if !document.pending_conversion() {
                    let (_, mut file) =
                        names.create(&folder, &safe_stem(&document.title), ".md")?;
                    file.write_all(document.markdown.as_bytes())?;
                    file.sync_all()?;
                }
                if let Some(kept) = &document.original {
                    let (stem, extension) = split_name(&kept.info.file_name);
                    let dir = folder.join(ORIGINALS);
                    std::fs::create_dir_all(&dir)?;
                    let mut reader = self.open_original(&document.id)?;
                    let (path, mut file) = originals.create(&dir, &safe_stem(stem), &extension)?;
                    let copied = io::copy(&mut reader, &mut file)
                        .map_err(original::read_error)
                        .and_then(|_| file.sync_all().map_err(VaultError::from));
                    if let Err(error) = copied {
                        drop(file);
                        let _ = std::fs::remove_file(&path);
                        return Err(error);
                    }
                }
                Ok(true)
            });
            match written {
                Ok(true) => exported.documents += 1,
                Ok(false) => {}
                Err(error) => exported.failed.push((summary.title.clone(), error)),
            }
        }
        Ok(exported)
    }
}

/// Names taken in one folder, compared ignoring case as Windows and macOS
/// do.
#[derive(Default)]
struct Names(HashSet<String>);

impl Names {
    /// `stem` + `extension`, or with " 2", " 3"… before the extension when
    /// that is taken.
    fn take(&mut self, stem: &str, extension: &str) -> String {
        let mut n = 1;
        loop {
            let name = if n == 1 {
                format!("{stem}{extension}")
            } else {
                format!("{stem} {n}{extension}")
            };
            if self.0.insert(name.to_lowercase()) {
                return name;
            }
            n += 1;
        }
    }

    /// A new file in `dir` under the next free name: one the file system
    /// already holds, as a name equal to another only once normalized,
    /// takes the next.
    fn create(&mut self, dir: &Path, stem: &str, extension: &str) -> io::Result<(PathBuf, File)> {
        loop {
            let path = dir.join(self.take(stem, extension));
            match create_new(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }
}

/// A new folder `name` in `into`, or `name 2`, `name 3`… when taken.
fn new_folder(into: &Path, name: &str) -> io::Result<PathBuf> {
    let mut n = 1;
    loop {
        let path = if n == 1 {
            into.join(name)
        } else {
            into.join(format!("{name} {n}"))
        };
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => n += 1,
            Err(error) => return Err(error),
        }
    }
}

/// `path` with its nearest existing ancestor resolved, so a folder not
/// made yet is placed where it will be.
fn resolved(path: &Path) -> io::Result<PathBuf> {
    let mut rest = Vec::new();
    let mut at = path;
    loop {
        match std::fs::canonicalize(at) {
            Ok(found) => return Ok(rest.iter().rev().fold(found, |path, part| path.join(part))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let (Some(parent), Some(name)) = (at.parent(), at.file_name()) else {
                    return Err(error);
                };
                rest.push(name.to_owned());
                at = parent;
            }
            Err(error) => return Err(error),
        }
    }
}

fn create_new(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

/// A file name's stem and its extension with the dot (empty when none).
fn split_name(name: &str) -> (&str, String) {
    match name.rsplit_once('.') {
        Some((stem, extension))
            if !stem.is_empty() && !extension.is_empty() && extension.len() <= 16 =>
        {
            (stem, format!(".{}", safe_extension(extension)))
        }
        _ => (name, String::new()),
    }
}

fn safe_extension(extension: &str) -> String {
    extension
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect()
}

/// `title` as a file name stem that Windows and macOS both accept: no
/// separators or reserved characters, no leading dot (hidden on macOS) or
/// trailing dot or space (dropped by Windows), no device name, and at most
/// [`MAX_STEM`] bytes. "Untitled" when nothing is left.
pub(crate) fn safe_stem(title: &str) -> String {
    let replaced: String = title
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let mut stem = replaced
        .trim()
        .trim_start_matches('.')
        .trim_end_matches(['.', ' '])
        .to_string();
    if stem.len() > MAX_STEM {
        let mut end = MAX_STEM;
        while !stem.is_char_boundary(end) {
            end -= 1;
        }
        stem.truncate(end);
        stem = stem.trim_end_matches(['.', ' ']).to_string();
    }
    if stem.is_empty() {
        return "Untitled".into();
    }
    // Windows reads a device name in the part before the first dot, with
    // trailing spaces, and COM and LPT take ¹, ² and ³ as digits too.
    let device = stem.split('.').next().unwrap_or_default().trim_end();
    let upper = device.to_uppercase();
    let mut chars = upper.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    let rest: Vec<char> = chars.collect();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (matches!(prefix.as_str(), "COM" | "LPT")
            && matches!(rest.as_slice(), [c] if c.is_ascii_digit() || matches!(c, '¹' | '²' | '³')));
    if reserved {
        stem.insert(0, '-');
    }
    stem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_become_names_windows_and_macos_accept() {
        assert_eq!(safe_stem("Smith v. Jones: brief"), "Smith v. Jones- brief");
        assert_eq!(safe_stem("a/b\\c|d?e*f\"g<h>"), "a-b-c-d-e-f-g-h-");
        assert_eq!(safe_stem("..hidden"), "hidden");
        assert_eq!(safe_stem("ends with dots. . "), "ends with dots");
        assert_eq!(safe_stem("   "), "Untitled");
        assert_eq!(safe_stem("con"), "-con");
        assert_eq!(safe_stem("COM1.notes"), "-COM1.notes");
        assert_eq!(safe_stem("lpt²"), "-lpt²");
        assert_eq!(safe_stem("nul .txt"), "-nul .txt");
        assert_eq!(safe_stem("COM10"), "COM10");
        assert_eq!(safe_stem("Console"), "Console");
        assert_eq!(safe_stem("line\nbreak"), "line-break");
        let long = safe_stem(&"ё".repeat(200));
        assert!(long.len() <= MAX_STEM && long.chars().all(|c| c == 'ё'));
    }

    #[test]
    fn names_are_unique_ignoring_case() {
        let mut names = Names::default();
        assert_eq!(names.take("Brief", ".md"), "Brief.md");
        assert_eq!(names.take("brief", ".md"), "brief 2.md");
        assert_eq!(names.take("Brief", ".md"), "Brief 3.md");
        assert_eq!(names.take("Brief", ".pdf"), "Brief.pdf");
    }

    #[test]
    fn file_names_split_at_their_last_dot() {
        assert_eq!(split_name("a.b.pdf"), ("a.b", ".pdf".into()));
        assert_eq!(split_name(".profile"), (".profile", String::new()));
        assert_eq!(split_name("README"), ("README", String::new()));
    }
}
