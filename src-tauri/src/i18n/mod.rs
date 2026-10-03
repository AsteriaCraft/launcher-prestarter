//! Interface languages (ADR 0007): the runtime's five, chosen from the OS locale, English otherwise. The message
//! files in `messages/` are the single source: the Svelte front end imports the same JSON, and Rust uses them for
//! native dialogs and errors when there is no window.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Be,
    En,
    Pl,
    Ru,
    Uk,
}

pub const ALL: [Lang; 5] = [Lang::Be, Lang::En, Lang::Pl, Lang::Ru, Lang::Uk];

impl Lang {
    pub const fn code(self) -> &'static str {
        match self {
            Lang::Be => "be",
            Lang::En => "en",
            Lang::Pl => "pl",
            Lang::Ru => "ru",
            Lang::Uk => "uk",
        }
    }

    const fn source(self) -> &'static str {
        match self {
            Lang::Be => include_str!("messages/be.json"),
            Lang::En => include_str!("messages/en.json"),
            Lang::Pl => include_str!("messages/pl.json"),
            Lang::Ru => include_str!("messages/ru.json"),
            Lang::Uk => include_str!("messages/uk.json"),
        }
    }

    /// From a BCP 47 or POSIX locale (`uk-UA`, `uk_UA.UTF-8`, `be-BY`, `pl`, `ru_RU`).
    pub fn from_locale(locale: &str) -> Lang {
        let primary = locale.split(['-', '_', '.', '@']).next().unwrap_or("").to_ascii_lowercase();
        match primary.as_str() {
            "uk" => Lang::Uk,
            "ru" => Lang::Ru,
            "be" => Lang::Be,
            "pl" => Lang::Pl,
            _ => Lang::En,
        }
    }

    /// The OS interface language.
    pub fn detect() -> Lang {
        sys_locale::get_locale().map(|l| Lang::from_locale(&l)).unwrap_or(Lang::En)
    }
}

type Table = HashMap<String, String>;

fn table(lang: Lang) -> &'static Table {
    static TABLES: OnceLock<HashMap<Lang, Table>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        ALL.iter()
            .map(|lang| {
                let value: serde_json::Value =
                    serde_json::from_str(lang.source()).expect("message files are valid JSON");
                let strings = value
                    .as_object()
                    .expect("message file is an object")
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                    .collect();
                (*lang, strings)
            })
            .collect()
    });
    &tables[&lang]
}

/// The message `key` in `lang` with `{name}` placeholders filled; falls back to English, then to the key itself.
pub fn t(lang: Lang, key: &str, params: &[(&str, &str)]) -> String {
    let template = table(lang).get(key).or_else(|| table(Lang::En).get(key)).map(String::as_str).unwrap_or(key);
    params.iter().fold(template.to_owned(), |text, (name, value)| text.replace(&format!("{{{name}}}"), value))
}

/// Bytes in the player's units (`120,3 МБ`): decimal megabytes like the OS file managers.
pub fn size(lang: Lang, bytes: u64) -> String {
    let (value, unit) = if bytes >= 1_000_000_000 {
        (bytes as f64 / 1e9, "unit.gb")
    } else if bytes >= 1_000_000 {
        (bytes as f64 / 1e6, "unit.mb")
    } else if bytes >= 1_000 {
        (bytes as f64 / 1e3, "unit.kb")
    } else {
        (bytes as f64, "unit.b")
    };
    let number = if value >= 100.0 || unit == "unit.b" { format!("{value:.0}") } else { format!("{value:.1}") };
    let number = if lang == Lang::En { number } else { number.replace('.', ",") };
    format!("{number} {}", t(lang, unit, &[]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn keys(lang: Lang) -> BTreeSet<String> {
        let value: serde_json::Value = serde_json::from_str(lang.source()).unwrap();
        value.as_object().unwrap().keys().cloned().collect()
    }

    #[test]
    fn every_language_has_every_key() {
        let english = keys(Lang::En);
        for lang in ALL {
            assert_eq!(keys(lang), english, "{}", lang.code());
        }
    }

    #[test]
    fn placeholders_match_english() {
        let english = table(Lang::En);
        let placeholders = |s: &str| -> BTreeSet<String> {
            s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(n, _)| n.to_owned())).collect()
        };
        for lang in ALL {
            for (key, text) in table(lang) {
                assert_eq!(placeholders(text), placeholders(&english[key]), "{} {key}", lang.code());
            }
        }
    }

    #[test]
    fn tips_exist_in_every_language() {
        for lang in ALL {
            let value: serde_json::Value = serde_json::from_str(lang.source()).unwrap();
            assert_eq!(value["tips"].as_array().unwrap().len(), 8, "{}", lang.code());
        }
    }

    #[test]
    fn locales() {
        assert_eq!(Lang::from_locale("uk-UA"), Lang::Uk);
        assert_eq!(Lang::from_locale("uk_UA.UTF-8"), Lang::Uk);
        assert_eq!(Lang::from_locale("be-BY"), Lang::Be);
        assert_eq!(Lang::from_locale("pl"), Lang::Pl);
        assert_eq!(Lang::from_locale("ru_RU"), Lang::Ru);
        assert_eq!(Lang::from_locale("de-DE"), Lang::En);
        assert_eq!(Lang::from_locale(""), Lang::En);
    }

    #[test]
    fn translation_and_fallback() {
        assert_eq!(t(Lang::Uk, "stage.jreDownload", &[]), "Завантажуємо Java");
        assert_eq!(t(Lang::En, "app.version", &[("version", "0.3.0")]), "version 0.3.0");
        assert_eq!(t(Lang::Pl, "no.such.key", &[]), "no.such.key");
    }

    #[test]
    fn sizes() {
        assert_eq!(size(Lang::Uk, 120_037_237), "120 МБ");
        assert_eq!(size(Lang::En, 8_503_901), "8.5 MB");
        assert_eq!(size(Lang::Uk, 8_503_901), "8,5 МБ");
        assert_eq!(size(Lang::En, 420), "420 B");
        assert_eq!(size(Lang::Pl, 1_500_000_000), "1,5 GB");
    }
}
