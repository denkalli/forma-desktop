//! Interface languages. Texts are written in English in the code and looked up in a table
//! for other languages: `tr("New design")`, or `trf("{n} layers", &[("n", "3")])` for texts
//! with values.

mod pt;

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// A language of the interface.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "pt")]
    Portuguese,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::English, Language::Portuguese];

    /// The language's own name, as shown in the language list.
    pub fn name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Portuguese => "Português",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Sets the language of the interface.
pub fn set_language(language: Language) {
    CURRENT.store(language as u8, Ordering::Relaxed);
}

pub fn language() -> Language {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Language::Portuguese,
        _ => Language::English,
    }
}

/// `text` in `language`. Texts missing from a table stay in English.
pub fn translate(language: Language, text: &'static str) -> &'static str {
    match language {
        Language::English => text,
        Language::Portuguese => pt::TABLE
            .binary_search_by(|(en, _)| en.cmp(&text))
            .map_or(text, |i| pt::TABLE[i].1),
    }
}

/// `text` in the current language.
pub fn tr(text: &'static str) -> &'static str {
    translate(language(), text)
}

/// Marks `text` for translation without translating it, for texts kept in constants and
/// passed to `tr` when shown.
pub const fn tr_noop(text: &'static str) -> &'static str {
    text
}

/// `text` in the current language, with each `{name}` replaced by its value.
pub fn trf(text: &'static str, values: &[(&str, &str)]) -> String {
    let mut out = tr(text).to_owned();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use super::*;

    #[test]
    fn english_is_the_default_and_tables_are_used() {
        assert_eq!(Language::default(), Language::English);
        assert_eq!(translate(Language::English, "Export"), "Export");
        assert_eq!(translate(Language::Portuguese, "Export"), "Exportar");
        assert_eq!(
            translate(Language::Portuguese, "not in the table"),
            "not in the table"
        );
    }

    #[test]
    fn the_portuguese_table_is_sorted_and_has_no_duplicates() {
        for pair in pt::TABLE.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "out of order: {:?} / {:?}",
                pair[0].0,
                pair[1].0
            );
        }
    }

    /// Every `tr("…")`, `trf("…")` and `tr_noop("…")` in the code has a Portuguese translation with the same
    /// placeholders, and the table has nothing that is never used.
    #[test]
    fn every_text_is_translated() {
        let mut used = BTreeSet::new();
        collect(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
            &mut used,
        );
        let table: BTreeSet<&str> = pt::TABLE.iter().map(|(en, _)| *en).collect();
        let missing: Vec<_> = used
            .iter()
            .filter(|t| !table.contains(t.as_str()))
            .collect();
        assert!(missing.is_empty(), "missing Portuguese for: {missing:#?}");
        let unused: Vec<_> = table.iter().filter(|t| !used.contains(**t)).collect();
        assert!(unused.is_empty(), "unused Portuguese entries: {unused:#?}");
        for (en, pt) in pt::TABLE {
            assert_eq!(
                placeholders(en),
                placeholders(pt),
                "placeholders differ in {en:?}"
            );
        }
    }

    fn placeholders(text: &str) -> BTreeSet<String> {
        text.split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}').map(|(name, _)| name.to_owned()))
            .collect()
    }

    /// Finds the string literals passed to `tr(` and `trf(` in the source files under `dir`.
    fn collect(dir: &Path, out: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                // The i18n module only has examples and the table itself.
                if !path.ends_with("i18n") {
                    collect(&path, out);
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                let source = std::fs::read_to_string(&path).unwrap();
                for call in ["tr(", "trf(", "tr_noop("] {
                    for (i, _) in source.match_indices(call) {
                        // Skip identifiers that merely end in `tr`/`trf`.
                        let before = source[..i].chars().last();
                        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        // rustfmt may move the text to the next line.
                        let rest = source[i + call.len()..].trim_start();
                        if let Some(rest) = rest.strip_prefix('"')
                            && let Some(text) = literal(rest)
                        {
                            out.insert(text);
                        }
                    }
                }
            }
        }
    }

    /// The contents of a Rust string literal whose opening quote was just consumed.
    fn literal(rest: &str) -> Option<String> {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(out),
                '\\' => match chars.next()? {
                    'n' => out.push('\n'),
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    other => {
                        out.push('\\');
                        out.push(other);
                    }
                },
                c => out.push(c),
            }
        }
        None
    }
}
