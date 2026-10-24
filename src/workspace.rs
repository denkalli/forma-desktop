//! A project folder: a folder of `.forma` files shown on the start screen.

use crate::i18n::tr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Folders where templates are looked up: the one installed with Forma (`share/forma/templates`
/// next to the program, `/app/share/...` in the Flatpak), then the user's own.
pub fn template_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(prefix) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent()?.parent().map(Path::to_path_buf))
    {
        dirs.push(prefix.join("share").join("forma").join("templates"));
    }
    dirs.extend(user_templates_dir());
    dirs
}

/// The user's template folder, where any `.forma` file becomes a template.
pub fn user_templates_dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("forma").join("templates"))
}

/// How deep subfolders are searched.
const MAX_DEPTH: usize = 4;

/// A `.forma` file found in the workspace.
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    /// Path relative to the workspace root, for display.
    pub relative: PathBuf,
    pub modified: SystemTime,
    pub title: String,
    pub pages: usize,
    /// PNG thumbnail.
    pub thumbnail: Option<Vec<u8>>,
    /// The file could not be read (for example, made by a newer version).
    pub error: Option<String>,
}

pub struct Workspace {
    pub root: PathBuf,
    pub files: Vec<FileEntry>,
    /// The folder exists. A folder on a disk that is not mounted stays in the list.
    pub available: bool,
}

impl Workspace {
    /// Reads a folder. A missing folder gives an empty, unavailable workspace.
    pub fn new(root: &Path) -> Self {
        let mut workspace = Self {
            root: root.to_path_buf(),
            files: Vec::new(),
            available: false,
        };
        workspace.rescan();
        workspace
    }

    pub fn name(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.display().to_string())
    }

    /// Reads the folder again, most recently modified first.
    pub fn rescan(&mut self) {
        self.available = self.root.is_dir();
        let mut files = Vec::new();
        collect(&self.root, &self.root, 0, &mut files);
        files.sort_by_key(|f| std::cmp::Reverse(f.modified));
        self.files = files;
    }

    /// A file name in the workspace that does not exist yet, based on `title`.
    pub fn unique_path(&self, title: &str) -> PathBuf {
        let base: String = title
            .chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | '\0') {
                    '-'
                } else {
                    c
                }
            })
            .collect();
        let base = if base.trim().is_empty() {
            tr("Untitled").to_owned()
        } else {
            base.trim().to_owned()
        };
        let mut path = self.root.join(format!("{base}.forma"));
        let mut n = 2;
        while path.exists() {
            path = self.root.join(format!("{base} {n}.forma"));
            n += 1;
        }
        path
    }
}

fn collect(root: &Path, dir: &Path, depth: usize, out: &mut Vec<FileEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if hidden {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            if depth < MAX_DEPTH {
                collect(root, &path, depth + 1, out);
            }
        } else if path
            .extension()
            .is_some_and(|e| e == libforma::format::EXTENSION)
        {
            out.push(read_entry(
                root,
                &path,
                meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            ));
        }
    }
}

fn read_entry(root: &Path, path: &Path, modified: SystemTime) -> FileEntry {
    let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    match libforma::format::open_preview(path) {
        Ok(preview) => {
            let m = preview.manifest;
            let error = (m.format_version > libforma::format::FORMAT_VERSION)
                .then(|| tr("Made by a newer version of Forma").to_owned());
            FileEntry {
                path: path.to_path_buf(),
                relative,
                modified,
                title: if m.title.is_empty() { stem } else { m.title },
                pages: m.pages.len(),
                thumbnail: preview.thumbnail,
                error,
            }
        }
        Err(err) => FileEntry {
            path: path.to_path_buf(),
            relative,
            modified,
            title: stem,
            pages: 0,
            thumbnail: None,
            error: Some(err.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_folder_stays_listed_but_unavailable() {
        let ws = Workspace::new(Path::new("/não/existe/forma-teste"));
        assert!(!ws.available);
        assert!(ws.files.is_empty());
        assert_eq!(ws.name(), "forma-teste");
    }

    #[test]
    fn new_files_get_unique_names() {
        let dir = std::env::temp_dir().join(format!("forma-ws-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ws = Workspace::new(&dir);
        assert!(ws.available);
        let first = ws.unique_path("Post / Junho");
        assert_eq!(first, dir.join("Post - Junho.forma"));
        std::fs::write(&first, b"").unwrap();
        assert_eq!(
            ws.unique_path("Post / Junho"),
            dir.join("Post - Junho 2.forma")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
