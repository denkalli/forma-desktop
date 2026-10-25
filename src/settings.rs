//! User preferences, stored as JSON in the XDG config directory
//! (`~/.config/forma/settings.json` on Linux, `%APPDATA%\forma\settings.json` on Windows).

use std::path::{Path, PathBuf};

use libforma::paint::Color;
use serde::{Deserialize, Serialize};

use crate::i18n::Language;
use crate::theme::ThemeMode;
use crate::theme::tokens::DEFAULT_ACCENT;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub theme: ThemeMode,
    pub accent: Color,
    /// Save automatically shortly after each edit, for documents that already have a file.
    pub autosave: bool,
    /// Rulers along the top and left of the canvas. Off by default; the right-click menu of
    /// the canvas turns them on. (Older settings stored `show_rulers`, which is ignored, so
    /// everyone starts with them off.)
    pub rulers: bool,
    /// Folders of designs shown on the start screen, in the order they were added.
    pub project_folders: Vec<PathBuf>,
    /// Older versions kept recent folders and the last one opened; they become project
    /// folders on load.
    #[serde(skip_serializing)]
    recent_folders: Vec<PathBuf>,
    #[serde(skip_serializing)]
    last_folder: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::English,
            theme: ThemeMode::Dark,
            accent: DEFAULT_ACCENT,
            autosave: true,
            rulers: false,
            project_folders: Vec::new(),
            recent_folders: Vec::new(),
            last_folder: None,
        }
    }
}

impl Settings {
    fn path() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("forma").join("settings.json"))
    }

    /// Loads the settings, falling back to defaults when there are none or they are invalid.
    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read(path).ok())
            .map(|data| Self::from_json(&data))
            .unwrap_or_default()
    }

    /// Parses saved settings, migrating older ones. Invalid data gives the defaults.
    fn from_json(data: &[u8]) -> Self {
        let mut settings: Self = serde_json::from_slice(data).unwrap_or_default();
        let legacy = settings.last_folder.take().into_iter();
        for folder in legacy.chain(std::mem::take(&mut settings.recent_folders)) {
            settings.add_folder(&folder);
        }
        settings
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }

    /// Adds a project folder at the end of the list. Returns `false` if it was already there.
    pub fn add_folder(&mut self, folder: &Path) -> bool {
        if self.project_folders.iter().any(|f| f == folder) {
            return false;
        }
        self.project_folders.push(folder.to_path_buf());
        true
    }

    /// Removes a folder from the list. Its files are left alone.
    pub fn remove_folder(&mut self, folder: &Path) {
        self.project_folders.retain(|f| f != folder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_recent_folders_become_project_folders() {
        let old = br#"{
            "theme": "light",
            "autosave": false,
            "recent_folders": ["/b", "/a", "/c"],
            "last_folder": "/a"
        }"#;
        let settings = Settings::from_json(old);
        assert_eq!(
            settings.project_folders,
            ["/a", "/b", "/c"].map(PathBuf::from)
        );
        assert!(!settings.autosave);
        // The old fields are not written back.
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("recent_folders") && !json.contains("last_folder"));
        assert_eq!(Settings::from_json(json.as_bytes()), settings);
    }

    #[test]
    fn folders_are_added_once_and_removed() {
        let mut settings = Settings::default();
        assert!(settings.add_folder(Path::new("/a")));
        assert!(!settings.add_folder(Path::new("/a")));
        assert!(settings.add_folder(Path::new("/b")));
        settings.remove_folder(Path::new("/a"));
        assert_eq!(settings.project_folders, [PathBuf::from("/b")]);
    }
}
