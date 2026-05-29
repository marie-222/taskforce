use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;
use gettext::Catalog;

static CATALOG: OnceLock<Option<Catalog>> = OnceLock::new();

pub fn init() -> Result<()> {
    if CATALOG.get().is_some() {
        return Ok(());
    }

    let catalog = load_catalog()?;
    let _ = CATALOG.set(catalog);
    Ok(())
}

pub fn tr(message: &str) -> String {
    CATALOG
        .get()
        .and_then(|catalog| catalog.as_ref())
        .map(|catalog| catalog.gettext(message).to_string())
        .unwrap_or_else(|| message.to_string())
}

fn load_catalog() -> Result<Option<Catalog>> {
    let locale_root = locale_root().join("locale");

    for locale in preferred_locale_candidates() {
        let path = locale_root
            .join(&locale)
            .join("LC_MESSAGES")
            .join("taskforce.mo");

        if path.is_file() {
            return Ok(Some(parse_catalog(&path)?));
        }
    }

    Ok(None)
}

fn parse_catalog(path: &Path) -> Result<Catalog> {
    let file = File::open(path)?;
    Ok(Catalog::parse(file)?)
}

fn locale_root() -> PathBuf {
    if let Ok(path) = std::env::var("TASKFORCE_LOCALE_ROOT") {
        return PathBuf::from(path);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn preferred_locale_candidates() -> Vec<String> {
    let mut candidates = Vec::new();

    for key in ["TASKFORCE_LOCALE", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            push_locale_candidates(&mut candidates, &value);
        }
    }

    candidates
}

fn push_locale_candidates(candidates: &mut Vec<String>, value: &str) {
    let normalized = value.trim();

    if !is_meaningful_locale(normalized) {
        return;
    }

    let without_encoding = normalized
        .split_once('.')
        .map(|(locale, _)| locale)
        .unwrap_or(normalized);
    let without_modifier = without_encoding
        .split_once('@')
        .map(|(locale, _)| locale)
        .unwrap_or(without_encoding);

    for candidate in [
        normalized,
        without_encoding,
        without_modifier,
        without_modifier
            .split_once('_')
            .map(|(language, _)| language)
            .unwrap_or(without_modifier),
    ] {
        if is_meaningful_locale(candidate)
            && !candidates.iter().any(|existing| existing == candidate)
        {
            candidates.push(candidate.to_string());
        }
    }
}

fn is_meaningful_locale(value: &str) -> bool {
    !value.is_empty() && value != "C" && value != "C.UTF-8" && value != "POSIX"
}

#[cfg(test)]
mod tests {
    use super::push_locale_candidates;

    #[test]
    fn builds_locale_fallback_candidates() {
        let mut candidates = Vec::new();
        push_locale_candidates(&mut candidates, "ja_JP.UTF-8");
        assert_eq!(candidates, vec!["ja_JP.UTF-8", "ja_JP", "ja"]);
    }

    #[test]
    fn ignores_non_meaningful_locales() {
        let mut candidates = Vec::new();
        push_locale_candidates(&mut candidates, "C.UTF-8");
        assert!(candidates.is_empty());
    }

    #[test]
    fn locale_candidates_do_not_duplicate_entries() {
        let mut candidates = Vec::new();
        push_locale_candidates(&mut candidates, "ja_JP.UTF-8");
        push_locale_candidates(&mut candidates, "ja_JP");
        assert_eq!(candidates, vec!["ja_JP.UTF-8", "ja_JP", "ja"]);
    }
}
