//! Every string a Discord user reads. Operator-facing logs and errors stay
//! in English and do not belong here.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    /// `en-US`, `fr`… anything untranslated falls back to `default`.
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

    pub fn watch_button(self) -> &'static str {
        self.pick("Tell me when it shows up", "Me prévenir quand il arrive")
    }

    pub fn watch_created(self, query: &str, per_day: u32, days: i64) -> String {
        match self {
            Lang::En => format!(
                "Watching **{query}**. I will look {per_day} times a day and post here as soon as \
                 something turns up, or give up after {days} days."
            ),
            Lang::Fr => format!(
                "Je surveille **{query}**. Je chercherai {per_day} fois par jour et je poste ici \
                 dès que quelque chose sort, sinon j'abandonne au bout de {days} jours."
            ),
        }
    }

    pub fn watch_already(self, query: &str) -> String {
        match self {
            Lang::En => format!("You are already watching **{query}**."),
            Lang::Fr => format!("Tu surveilles déjà **{query}**."),
        }
    }

    pub fn watch_too_many(self, max: usize) -> String {
        match self {
            Lang::En => {
                format!("You already have {max} searches running. Stop one with `/watchlist`.")
            }
            Lang::Fr => {
                format!("Tu as déjà {max} recherches en cours. Arrêtes-en une avec `/watchlist`.")
            }
        }
    }

    pub fn watch_found(self, query: &str) -> String {
        match self {
            Lang::En => format!(
                "**{query}** turned up. Run the search again to download it — I will drop it \
                 from your watchlist once you do."
            ),
            Lang::Fr => format!(
                "**{query}** est sorti. Relance la recherche pour le télécharger — je le \
                 retirerai de tes veilles à ce moment-là."
            ),
        }
    }

    pub fn watch_fulfilled(self, query: &str) -> String {
        match self {
            Lang::En => format!("Removed **{query}** from your watchlist."),
            Lang::Fr => format!("J'ai retiré **{query}** de tes veilles."),
        }
    }

    /// Shown in the listing for a watch whose book has already been found.
    pub fn watchlist_waiting(self) -> &'static str {
        self.pick(
            "found · waiting for your download",
            "trouvé · en attente de téléchargement",
        )
    }

    pub fn watch_gave_up(self, query: &str, days: i64) -> String {
        match self {
            Lang::En => format!("Gave up on **{query}**: nothing turned up in {days} days."),
            Lang::Fr => format!("J'abandonne **{query}** : rien n'est sorti en {days} jours."),
        }
    }

    pub fn watchlist_title(self) -> &'static str {
        self.pick("Your standing searches", "Tes recherches en cours")
    }

    pub fn watchlist_empty(self) -> &'static str {
        self.pick(
            "You have no standing search. Run a search that finds nothing to start one.",
            "Tu n'as aucune recherche en cours. Lances-en une qui ne trouve rien pour en créer une.",
        )
    }

    pub fn watchlist_entry(self, checks: u32, days_left: i64) -> String {
        match self {
            Lang::En => format!("checked {checks} times · {days_left} days left"),
            Lang::Fr => format!("vérifiée {checks} fois · encore {days_left} jours"),
        }
    }

    pub fn watchlist_admin_title(self) -> &'static str {
        self.pick("All standing searches", "Toutes les recherches en cours")
    }

    pub fn watchlist_admin_empty(self) -> &'static str {
        self.pick(
            "Nobody on this server has a standing search.",
            "Personne sur ce serveur n'a de recherche en cours.",
        )
    }

    /// Discord does not render mentions inside a select menu option, so the
    /// moderator view falls back to the raw id there.
    pub fn watchlist_owner(self, user_id: u64) -> String {
        match self {
            Lang::En => format!("owner {user_id}"),
            Lang::Fr => format!("propriétaire {user_id}"),
        }
    }

    pub fn watchlist_more(self, hidden: usize) -> String {
        match self {
            Lang::En => format!("\n\n_…and {hidden} more, not shown._"),
            Lang::Fr => format!("\n\n_…et {hidden} autre(s), non affichée(s)._"),
        }
    }

    pub fn watch_stopped_for(self, query: &str, user_id: u64) -> String {
        match self {
            Lang::En => format!("Stopped **{query}** for <@{user_id}>."),
            Lang::Fr => format!("J'ai arrêté **{query}** pour <@{user_id}>."),
        }
    }

    pub fn watchlist_placeholder(self) -> &'static str {
        self.pick("Pick a search to stop", "Choisis une recherche à arrêter")
    }

    pub fn watch_stopped(self, query: &str) -> String {
        match self {
            Lang::En => format!("Stopped watching **{query}**."),
            Lang::Fr => format!("J'arrête de surveiller **{query}**."),
        }
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
    fn a_language_round_trips_through_json() {
        for lang in [Lang::En, Lang::Fr] {
            let encoded = serde_json::to_string(&lang).unwrap();
            assert_eq!(serde_json::from_str::<Lang>(&encoded).unwrap(), lang);
        }
        assert_eq!(serde_json::to_string(&Lang::Fr).unwrap(), "\"fr\"");
    }

    #[test]
    fn a_language_renders_as_its_code() {
        assert_eq!(Lang::En.to_string(), "en");
        assert_eq!(Lang::Fr.to_string(), "fr");
    }

    #[test]
    fn every_string_differs_between_the_two_languages() {
        // A translation that silently falls back to English would read as a bug
        // to a French user, and nothing else would catch it.
        let pairs: Vec<(String, String)> = vec![
            (Lang::En.no_results("q"), Lang::Fr.no_results("q")),
            (Lang::En.more_in_menu(3), Lang::Fr.more_in_menu(3)),
            (
                Lang::En.release_summary("1 kB", 4, "i"),
                Lang::Fr.release_summary("1 kB", 4, "i"),
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
            (
                Lang::En.watch_button().into(),
                Lang::Fr.watch_button().into(),
            ),
            (
                Lang::En.watch_created("q", 4, 30),
                Lang::Fr.watch_created("q", 4, 30),
            ),
            (Lang::En.watch_already("q"), Lang::Fr.watch_already("q")),
            (Lang::En.watch_too_many(10), Lang::Fr.watch_too_many(10)),
            (Lang::En.watch_found("q"), Lang::Fr.watch_found("q")),
            (Lang::En.watch_fulfilled("q"), Lang::Fr.watch_fulfilled("q")),
            (
                Lang::En.watch_gave_up("q", 30),
                Lang::Fr.watch_gave_up("q", 30),
            ),
            (Lang::En.watch_stopped("q"), Lang::Fr.watch_stopped("q")),
            (
                Lang::En.watchlist_title().into(),
                Lang::Fr.watchlist_title().into(),
            ),
            (
                Lang::En.watchlist_empty().into(),
                Lang::Fr.watchlist_empty().into(),
            ),
            (
                Lang::En.watchlist_entry(3, 7),
                Lang::Fr.watchlist_entry(3, 7),
            ),
            (
                Lang::En.watchlist_waiting().into(),
                Lang::Fr.watchlist_waiting().into(),
            ),
            (
                Lang::En.watchlist_placeholder().into(),
                Lang::Fr.watchlist_placeholder().into(),
            ),
            (
                Lang::En.watchlist_admin_title().into(),
                Lang::Fr.watchlist_admin_title().into(),
            ),
            (
                Lang::En.watchlist_admin_empty().into(),
                Lang::Fr.watchlist_admin_empty().into(),
            ),
            (Lang::En.watchlist_owner(7), Lang::Fr.watchlist_owner(7)),
            (Lang::En.watchlist_more(3), Lang::Fr.watchlist_more(3)),
            (
                Lang::En.watch_stopped_for("q", 7),
                Lang::Fr.watch_stopped_for("q", 7),
            ),
        ];

        for (en, fr) in pairs {
            assert_ne!(en, fr, "this string was left untranslated: {en}");
        }
    }

    #[test]
    fn interpolated_values_survive_translation() {
        let cases: Vec<(String, Vec<&str>)> = vec![
            (Lang::Fr.no_results("dune"), vec!["dune"]),
            (Lang::Fr.more_in_menu(7), vec!["7"]),
            (
                Lang::Fr.release_summary("930 kB", 40, "C411"),
                vec!["930 kB", "40", "C411"],
            ),
            (Lang::Fr.category_missing("ebooks"), vec!["ebooks"]),
            (
                Lang::Fr.watch_created("dune", 4, 30),
                vec!["dune", "4", "30"],
            ),
            (Lang::Fr.watch_already("dune"), vec!["dune"]),
            (Lang::Fr.watch_too_many(10), vec!["10"]),
            (Lang::Fr.watch_found("dune"), vec!["dune"]),
            (Lang::Fr.watch_fulfilled("dune"), vec!["dune"]),
            (Lang::Fr.watch_gave_up("dune", 30), vec!["dune", "30"]),
            (Lang::Fr.watch_stopped("dune"), vec!["dune"]),
            (Lang::Fr.watchlist_entry(3, 7), vec!["3", "7"]),
            (Lang::Fr.watchlist_owner(4242), vec!["4242"]),
            (Lang::Fr.watchlist_more(3), vec!["3"]),
            (Lang::Fr.watch_stopped_for("dune", 7), vec!["dune", "<@7>"]),
        ];

        for (rendered, expected) in cases {
            for value in expected {
                assert!(
                    rendered.contains(value),
                    "{value} is missing from: {rendered}"
                );
            }
        }
    }

    #[test]
    fn destination_is_intentionally_identical_in_both_languages() {
        assert_eq!(Lang::En.field_destination(), Lang::Fr.field_destination());
    }
}
