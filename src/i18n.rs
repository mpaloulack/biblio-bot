//! User-facing strings, in English and French.
//!
//! Discord tells us which locale the invoking user has selected, so the same
//! bot answers each person in their own language. Code, logs and errors aimed at
//! the operator stay in English; only what a Discord user reads is translated.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    /// Maps a Discord locale (`en-US`, `en-GB`, `fr`…) to a supported language,
    /// falling back to `default` for every locale we do not translate.
    pub fn from_locale(locale: Option<&str>, default: Lang) -> Self {
        match locale {
            Some(l) if l.starts_with("fr") => Lang::Fr,
            Some(l) if l.starts_with("en") => Lang::En,
            _ => default,
        }
    }

    fn pick(self, en: &'static str, fr: &'static str) -> &'static str {
        match self {
            Lang::En => en,
            Lang::Fr => fr,
        }
    }

    pub fn no_results(self, query: &str) -> String {
        match self {
            Lang::En => format!("No result for **{query}**."),
            Lang::Fr => format!("Aucun résultat pour **{query}**."),
        }
    }

    pub fn more_in_menu(self, hidden: usize) -> String {
        match self {
            Lang::En => format!("\n\n_…and {hidden} more in the menu below._"),
            Lang::Fr => format!("\n\n_…et {hidden} autre(s) dans le menu ci-dessous._"),
        }
    }

    /// Short summary shown under each menu entry.
    pub fn release_summary(self, size: &str, seeders: u32, indexer: &str) -> String {
        match self {
            Lang::En => format!("{size} · {seeders} seeders · {indexer}"),
            Lang::Fr => format!("{size} · {seeders} sources · {indexer}"),
        }
    }

    pub fn menu_placeholder(self) -> &'static str {
        self.pick(
            "Pick a release to download",
            "Choisis le fichier à télécharger",
        )
    }

    pub fn selection_timed_out(self) -> &'static str {
        self.pick("⏱️ Selection timed out.", "⏱️ Sélection expirée.")
    }

    pub fn invalid_selection(self) -> &'static str {
        self.pick("Invalid selection.", "Sélection invalide.")
    }

    pub fn added_title(self) -> &'static str {
        self.pick("📥 Sent to qBittorrent", "📥 Envoyé à qBittorrent")
    }

    pub fn failed_title(self) -> &'static str {
        self.pick("❌ Could not add the download", "❌ Échec de l'ajout")
    }

    pub fn status_title(self) -> &'static str {
        self.pick("Service status", "État des services")
    }

    pub fn field_category(self) -> &'static str {
        self.pick("Category", "Catégorie")
    }

    pub fn field_destination(self) -> &'static str {
        self.pick("Destination", "Destination")
    }

    pub fn field_size(self) -> &'static str {
        self.pick("Size", "Taille")
    }

    pub fn default_folder(self) -> &'static str {
        self.pick(
            "qBittorrent default folder",
            "dossier par défaut de qBittorrent",
        )
    }

    pub fn no_save_path(self) -> &'static str {
        self.pick(
            "⚠️ no save path set (qBittorrent default folder)",
            "⚠️ aucun chemin configuré (dossier par défaut de qBittorrent)",
        )
    }

    pub fn category_missing(self, category: &str) -> String {
        match self {
            Lang::En => format!("⚠️ category `{category}` does not exist in qBittorrent"),
            Lang::Fr => format!("⚠️ catégorie `{category}` absente de qBittorrent"),
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        })
    }
}

impl FromStr for Lang {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "en" | "en-us" | "en-gb" | "english" => Ok(Lang::En),
            "fr" | "fr-fr" | "french" | "francais" | "français" => Ok(Lang::Fr),
            other => Err(format!(
                "unsupported language: {other} (expected `en` or `fr`)"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_default() {
        assert_eq!(Lang::default(), Lang::En);
    }

    #[test]
    fn discord_locales_map_to_a_language() {
        assert_eq!(Lang::from_locale(Some("fr"), Lang::En), Lang::Fr);
        assert_eq!(Lang::from_locale(Some("en-US"), Lang::Fr), Lang::En);
        assert_eq!(Lang::from_locale(Some("en-GB"), Lang::Fr), Lang::En);
    }

    #[test]
    fn an_untranslated_locale_falls_back_to_the_configured_default() {
        assert_eq!(Lang::from_locale(Some("de"), Lang::Fr), Lang::Fr);
        assert_eq!(Lang::from_locale(Some("ja"), Lang::En), Lang::En);
    }

    #[test]
    fn a_missing_locale_falls_back_to_the_configured_default() {
        assert_eq!(Lang::from_locale(None, Lang::Fr), Lang::Fr);
        assert_eq!(Lang::from_locale(None, Lang::En), Lang::En);
    }

    #[test]
    fn parsing_accepts_the_spellings_a_user_would_write() {
        for input in ["en", "EN", " en-US ", "english"] {
            assert_eq!(input.parse::<Lang>().unwrap(), Lang::En, "for {input}");
        }
        for input in ["fr", "FR", "fr-FR", "french", "français"] {
            assert_eq!(input.parse::<Lang>().unwrap(), Lang::Fr, "for {input}");
        }
    }

    #[test]
    fn parsing_rejects_an_unsupported_language_and_says_which() {
        let error = "de".parse::<Lang>().unwrap_err();
        assert!(error.contains("de"), "got: {error}");
        assert!(error.contains("`en` or `fr`"), "got: {error}");
    }

    #[test]
    fn a_language_renders_as_its_code() {
        assert_eq!(Lang::En.to_string(), "en");
        assert_eq!(Lang::Fr.to_string(), "fr");
    }

    #[test]
    fn every_string_differs_between_the_two_languages() {
        // Guards against a translation silently falling back to English.
        let pairs: Vec<(String, String)> = vec![
            (Lang::En.no_results("q"), Lang::Fr.no_results("q")),
            (Lang::En.more_in_menu(3), Lang::Fr.more_in_menu(3)),
            (
                Lang::En.release_summary("1 kB", 4, "idx"),
                Lang::Fr.release_summary("1 kB", 4, "idx"),
            ),
            (
                Lang::En.menu_placeholder().into(),
                Lang::Fr.menu_placeholder().into(),
            ),
            (
                Lang::En.selection_timed_out().into(),
                Lang::Fr.selection_timed_out().into(),
            ),
            (
                Lang::En.invalid_selection().into(),
                Lang::Fr.invalid_selection().into(),
            ),
            (Lang::En.added_title().into(), Lang::Fr.added_title().into()),
            (
                Lang::En.failed_title().into(),
                Lang::Fr.failed_title().into(),
            ),
            (
                Lang::En.status_title().into(),
                Lang::Fr.status_title().into(),
            ),
            (
                Lang::En.field_category().into(),
                Lang::Fr.field_category().into(),
            ),
            (Lang::En.field_size().into(), Lang::Fr.field_size().into()),
            (
                Lang::En.default_folder().into(),
                Lang::Fr.default_folder().into(),
            ),
            (
                Lang::En.no_save_path().into(),
                Lang::Fr.no_save_path().into(),
            ),
            (
                Lang::En.category_missing("c"),
                Lang::Fr.category_missing("c"),
            ),
        ];

        for (en, fr) in pairs {
            assert_ne!(en, fr, "this string was left untranslated: {en}");
        }
    }

    #[test]
    fn interpolated_values_survive_translation() {
        assert!(Lang::Fr.no_results("dune").contains("dune"));
        assert!(Lang::Fr.more_in_menu(7).contains('7'));
        assert!(Lang::Fr.category_missing("ebooks").contains("ebooks"));
        assert!(
            Lang::Fr
                .release_summary("930 kB", 40, "C411")
                .contains("930 kB")
        );
        assert!(
            Lang::Fr
                .release_summary("930 kB", 40, "C411")
                .contains("C411")
        );
    }

    #[test]
    fn destination_is_intentionally_identical_in_both_languages() {
        assert_eq!(Lang::En.field_destination(), Lang::Fr.field_destination());
    }
}
