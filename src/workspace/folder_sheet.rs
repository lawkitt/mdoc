//! The folder sheet (ADR 0075): what a dropped folder shows before anything
//! opens. Plain state, no GPUI: the files with their ticks and the 20-file
//! cap, the keyboard cursor, and the folders waiting for their turn.
use mdoc_store::session_store;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

/// One folder drop opens at most this many new tabs.
pub(super) const CAP: usize = 20;
/// Said when a tick would go over the cap.
pub(super) const CAP_NOTE: &str = "Up to 20 files at a time";

/// A dropped folder: its supported files, read off the UI thread.
pub(super) struct DroppedFolder {
    /// The folder's identity, to match a case's members by.
    pub path: PathBuf,
    pub name: String,
    pub files: Vec<PathBuf>,
    pub skipped_folders: usize,
    pub skipped_hidden: usize,
}
impl DroppedFolder {
    /// The folder's own supported files, sorted by name; subfolders skipped.
    pub fn read(path: &Path, supported: impl Fn(&Path) -> bool) -> Option<Self> {
        if !path.is_dir() {
            return None;
        }
        let mut files = Vec::new();
        let mut skipped_folders = 0;
        let mut skipped_hidden = 0;
        for entry in std::fs::read_dir(path).ok()?.flatten() {
            let entry = entry.path();
            let hidden = entry
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'));
            if entry.is_dir() {
                skipped_folders += usize::from(!hidden);
            } else if supported(&entry) {
                if hidden {
                    skipped_hidden += 1;
                } else {
                    files.push(session_store::identity(&entry));
                }
            }
        }
        files.sort_by_key(|f| f.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        Some(Self {
            path: session_store::identity(path),
            name: path
                .file_name()
                .map_or_else(|| "Folder".into(), |n| n.to_string_lossy().into_owned()),
            files,
            skipped_folders,
            skipped_hidden,
        })
    }

    /// "2 subfolders, 1 hidden file skipped", or nothing when none were.
    pub fn skipped(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.skipped_folders > 0 {
            parts.push(plural(self.skipped_folders, "subfolder", "subfolders"));
        }
        if self.skipped_hidden > 0 {
            parts.push(plural(self.skipped_hidden, "hidden file", "hidden files"));
        }
        (!parts.is_empty()).then(|| parts.join(", ") + " skipped")
    }

    /// The notice for a folder with nothing to open: no sheet shows.
    pub fn empty_notice(&self) -> String {
        let mut notes = vec![format!("“{}” has no supported files", self.name)];
        if self.skipped_folders > 0 {
            notes.push(format!(
                "{} inside “{}” skipped",
                plural(self.skipped_folders, "subfolder", "subfolders"),
                self.name
            ));
        }
        if self.skipped_hidden > 0 {
            notes.push(format!(
                "{} skipped",
                plural(self.skipped_hidden, "hidden file", "hidden files")
            ));
        }
        notes.join("; ") + "."
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Whether a file of the folder already has a tab.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Status {
    New,
    /// Open without a case: it joins a case the sheet makes or names.
    Open,
    /// Open in a case, by name when the case is loaded. It stays there.
    InCase(Option<String>),
}
impl Status {
    pub fn label(&self) -> Option<String> {
        match self {
            Self::New => None,
            Self::Open => Some("Already open".into()),
            Self::InCase(Some(name)) => Some(format!("In “{name}”")),
            Self::InCase(None) => Some("In a locked case".into()),
        }
    }
}

pub(super) struct Row {
    pub path: PathBuf,
    pub name: String,
    pub status: Status,
    pub ticked: bool,
}

/// The loaded case the primary button adds to.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Target {
    pub id: String,
    pub name: String,
}

/// A folder waiting for its sheet: the drop's case header, if any, and
/// where in the sidebar the drop landed.
pub(super) struct Pending {
    pub folder: DroppedFolder,
    pub case: Option<String>,
    pub at: Option<usize>,
}

pub(super) struct Sheet {
    /// Which shown sheet this is, so a late Keychain answer for one sheet
    /// never acts on the next.
    id: u64,
    name: String,
    skipped: Option<String>,
    rows: Vec<Row>,
    target: Option<Target>,
    cursor: usize,
    refused: bool,
    at: Option<usize>,
}
impl Sheet {
    /// Every file starts ticked, except new files past the cap.
    pub fn new(
        folder: &DroppedFolder,
        statuses: Vec<Status>,
        target: Option<Target>,
        at: Option<usize>,
    ) -> Self {
        let mut new = 0;
        let rows = folder
            .files
            .iter()
            .zip(statuses)
            .map(|(path, status)| {
                let ticked = status != Status::New || {
                    new += 1;
                    new <= CAP
                };
                Row {
                    name: path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    path: path.clone(),
                    status,
                    ticked,
                }
            })
            .collect();
        Self {
            id: 0,
            name: folder.name.clone(),
            skipped: folder.skipped(),
            rows,
            target,
            cursor: 0,
            refused: false,
            at,
        }
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn skipped(&self) -> Option<&str> {
        self.skipped.as_deref()
    }
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }
    pub fn target(&self) -> Option<&Target> {
        self.target.as_ref()
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn at(&self) -> Option<usize> {
        self.at
    }
    /// A tick went over the cap since the last change.
    pub fn refused(&self) -> bool {
        self.refused
    }
    /// More new files than the cap: some start unticked.
    pub fn over_cap(&self) -> bool {
        self.rows.iter().filter(|r| r.status == Status::New).count() > CAP
    }
    pub fn primary_label(&self) -> String {
        match &self.target {
            Some(target) => format!("Add to “{}”", target.name),
            None => "Open as case".into(),
        }
    }
    pub fn ticked(&self) -> Vec<PathBuf> {
        self.rows
            .iter()
            .filter(|r| r.ticked)
            .map(|r| r.path.clone())
            .collect()
    }
    pub fn ticked_count(&self) -> usize {
        self.rows.iter().filter(|r| r.ticked).count()
    }
    fn new_ticked(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.ticked && r.status == Status::New)
            .count()
    }
    /// Tick or untick row `index`, which becomes the cursor. A new file past
    /// the cap stays unticked and the sheet says why.
    pub fn toggle(&mut self, index: usize) {
        let new_ticked = self.new_ticked();
        let Some(row) = self.rows.get_mut(index) else {
            return;
        };
        self.cursor = index;
        if !row.ticked && row.status == Status::New && new_ticked >= CAP {
            self.refused = true;
            return;
        }
        row.ticked = !row.ticked;
        self.refused = false;
    }
    /// ↑ / ↓: the row the cursor moves to, if any.
    pub fn step(&mut self, down: bool) -> Option<usize> {
        let next = if down {
            (self.cursor + 1 < self.rows.len()).then_some(self.cursor + 1)
        } else {
            self.cursor.checked_sub(1)
        }?;
        self.cursor = next;
        Some(next)
    }
    pub fn set_cursor(&mut self, index: usize) {
        if index < self.rows.len() {
            self.cursor = index;
        }
    }
}

/// The sheet on screen and the folders after it, one at a time.
#[derive(Default)]
pub(super) struct FolderSheets {
    shown: Option<Sheet>,
    queue: VecDeque<Pending>,
    shown_count: u64,
    /// The shown sheet's case action waits on the Keychain explainer.
    key_wait: bool,
}
impl FolderSheets {
    pub fn push(&mut self, pending: Pending) {
        self.queue.push_back(pending);
    }
    /// The next folder, once no sheet shows.
    pub fn next(&mut self) -> Option<Pending> {
        if self.shown.is_some() {
            return None;
        }
        self.queue.pop_front()
    }
    pub fn show(&mut self, mut sheet: Sheet) {
        self.shown_count += 1;
        sheet.id = self.shown_count;
        self.shown = Some(sheet);
        self.key_wait = false;
    }
    pub fn sheet(&self) -> Option<&Sheet> {
        self.shown.as_ref()
    }
    pub fn sheet_mut(&mut self) -> Option<&mut Sheet> {
        self.shown.as_mut()
    }
    pub fn open(&self) -> bool {
        self.shown.is_some()
    }
    /// The answered sheet, gone from the screen.
    pub fn take(&mut self) -> Option<Sheet> {
        self.key_wait = false;
        self.shown.take()
    }
    pub fn waiting(&self) -> bool {
        self.key_wait && self.shown.is_some()
    }
    pub fn set_waiting(&mut self, waiting: bool) {
        self.key_wait = waiting;
    }
    /// Folders still waiting for a sheet.
    #[cfg(test)]
    pub fn queued(&self) -> usize {
        self.queue.len()
    }
}
