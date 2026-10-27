//! Interface language. English text is the lookup key for `t!`; `locales/*.yml`
//! translate it, and missing translations fall back to the English text.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Follows the operating system's preferred language.
    #[default]
    System,
    English,
    Korean,
}
impl Language {
    pub const ALL: [Self; 3] = [Self::System, Self::English, Self::Korean];
    /// Language names are shown in their own language so anyone can find theirs.
    #[must_use]
    pub fn name(self) -> std::borrow::Cow<'static, str> {
        match self {
            Self::System => rust_i18n::t!("System language"),
            Self::English => "English".into(),
            Self::Korean => "한국어".into(),
        }
    }
    #[must_use]
    pub fn locale(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Korean => "ko",
            Self::System => system_locale(sys_locale::get_locale().as_deref()),
        }
    }
    /// Sets the language for all later `t!` lookups.
    pub fn apply(self) {
        rust_i18n::set_locale(self.locale());
    }
}
fn system_locale(preferred: Option<&str>) -> &'static str {
    match preferred {
        Some(tag)
            if tag
                .split(['-', '_', '.'])
                .next()
                .is_some_and(|l| l.eq_ignore_ascii_case("ko")) =>
        {
            "ko"
        }
        _ => "en",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    #[test]
    fn system_language_picks_korean_only_for_korean_locales() {
        assert_eq!(system_locale(Some("ko-KR")), "ko");
        assert_eq!(system_locale(Some("ko")), "ko");
        assert_eq!(system_locale(Some("KO_kr.UTF-8")), "ko");
        assert_eq!(system_locale(Some("en-US")), "en");
        assert_eq!(system_locale(Some("kok-IN")), "en");
        assert_eq!(system_locale(Some("k")), "en");
        assert_eq!(system_locale(None), "en");
        let saved: Language = serde_json::from_str("\"korean\"").unwrap();
        assert_eq!(saved, Language::Korean);
    }

    /// Reads `"key":` / `  ko: "value"` pairs. Locale files keep this exact
    /// double-quoted layout so keys can be checked without a YAML parser.
    fn translations() -> BTreeMap<String, String> {
        let mut entries = BTreeMap::new();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        for file in std::fs::read_dir(dir).unwrap() {
            let path = file.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            let mut key = None;
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix('"') {
                    let end = rest
                        .rfind("\":")
                        .unwrap_or_else(|| panic!("{path:?}: {line}"));
                    let quoted = format!("\"{}\"", &rest[..end]);
                    key = Some(serde_json::from_str::<String>(&quoted).unwrap());
                } else if let Some(value) = line.strip_prefix("  ko: ") {
                    let key = key.take().unwrap_or_else(|| panic!("{path:?}: {line}"));
                    let value: String = serde_json::from_str(value)
                        .unwrap_or_else(|e| panic!("{path:?}: {line}: {e}"));
                    assert!(
                        entries.insert(key.clone(), value).is_none(),
                        "{key:?} is translated twice"
                    );
                } else {
                    assert!(
                        line.is_empty() || line.starts_with('#') || line == "_version: 2",
                        "{path:?}: unexpected line {line}"
                    );
                }
            }
            assert!(key.is_none(), "{path:?}: last key has no translation");
        }
        entries
    }

    /// Every literal passed to `t!` in the source.
    fn keys_in_source() -> BTreeSet<String> {
        fn walk(dir: &Path, keys: &mut BTreeSet<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, keys);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    // Split so this scanner does not find its own pattern.
                    for (start, _) in text.match_indices(concat!("t!", "(\"")) {
                        // Skip macros that merely end in `t!`, such as `format!`.
                        if text[..start]
                            .chars()
                            .next_back()
                            .is_some_and(|c| c.is_alphanumeric() || c == '_')
                        {
                            continue;
                        }
                        let literal = &text[start + 3..];
                        let mut end = 1;
                        let bytes = literal.as_bytes();
                        while bytes[end] != b'"' {
                            end += if bytes[end] == b'\\' { 2 } else { 1 };
                        }
                        let key: String = serde_json::from_str(&literal[..=end])
                            .unwrap_or_else(|e| panic!("{path:?}: {}: {e}", &literal[..=end]));
                        keys.insert(key);
                    }
                }
            }
        }
        let mut keys = BTreeSet::new();
        walk(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut keys,
        );
        keys
    }

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split("%{")
            .skip(1)
            .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
            .collect()
    }

    #[test]
    fn every_interface_text_has_a_matching_korean_translation() {
        let translations = translations();
        let keys = keys_in_source();
        let missing: Vec<_> = keys
            .iter()
            .filter(|k| !translations.contains_key(*k))
            .collect();
        assert!(missing.is_empty(), "Untranslated: {missing:#?}");
        let unused: Vec<_> = translations.keys().filter(|k| !keys.contains(*k)).collect();
        assert!(
            unused.is_empty(),
            "Translations no longer used: {unused:#?}"
        );
        for (key, value) in &translations {
            assert!(!value.trim().is_empty(), "{key:?} has an empty translation");
            assert_eq!(
                placeholders(key),
                placeholders(value),
                "{key:?} → {value:?}"
            );
            // The compiled table must agree with the file layout read above.
            assert_eq!(rust_i18n::t!(key.as_str(), locale = "ko"), value.as_str());
            assert_eq!(rust_i18n::t!(key.as_str(), locale = "en"), key.as_str());
        }
    }
}
