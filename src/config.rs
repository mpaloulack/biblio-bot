use crate::i18n::Lang;
use anyhow::{Context as _, Result, bail};
use std::env;

/// Discord caps a select menu at 25 entries.
const MAX_SELECTABLE: usize = 25;
/// Newznab category for Books/EBook.
const DEFAULT_CATEGORY: u32 = 7020;

/// Everything the bot needs, read from the environment once at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub discord_token: String,
    /// When set, commands register in this guild only — instantly, instead of
    /// the global registration Discord can take an hour to propagate.
    pub guild_id: Option<u64>,
    pub prowlarr_url: String,
    pub prowlarr_api_key: String,
    pub qbit_url: String,
    pub qbit_user: Option<String>,
    pub qbit_pass: Option<String>,
    /// qBittorrent category the downloads are filed under.
    pub qbit_category: String,
    /// Newznab categories to search.
    pub search_categories: Vec<u32>,
    /// How many results to offer.
    pub max_results: usize,
    /// Language used when a user's Discord locale is one we do not translate.
    pub default_locale: Lang,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// Reads from an arbitrary source, so tests never have to mutate the
    /// process environment (which they share, and would race on).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let get = |key: &str| {
            lookup(key)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let required =
            |key: &str| get(key).with_context(|| format!("missing environment variable: {key}"));

        let max_results = get("MAX_RESULTS")
            .map(|v| v.parse::<usize>().context("MAX_RESULTS must be an integer"))
            .transpose()?
            .unwrap_or(MAX_SELECTABLE);
        if !(1..=MAX_SELECTABLE).contains(&max_results) {
            bail!("MAX_RESULTS must be between 1 and {MAX_SELECTABLE} (Discord select menu limit)");
        }

        Ok(Self {
            discord_token: required("DISCORD_TOKEN")?,
            guild_id: get("DISCORD_GUILD_ID")
                .map(|v| {
                    v.parse::<u64>()
                        .context("DISCORD_GUILD_ID must be a numeric id")
                })
                .transpose()?,
            prowlarr_url: strip_trailing_slash(&required("PROWLARR_URL")?),
            prowlarr_api_key: required("PROWLARR_API_KEY")?,
            qbit_url: strip_trailing_slash(&required("QBIT_URL")?),
            qbit_user: get("QBIT_USER"),
            qbit_pass: get("QBIT_PASS"),
            qbit_category: get("QBIT_CATEGORY").unwrap_or_else(|| "ebooks".to_owned()),
            search_categories: parse_categories(get("SEARCH_CATEGORIES").as_deref())?,
            max_results,
            default_locale: get("DEFAULT_LOCALE")
                .map(|v| {
                    v.parse::<Lang>()
                        .map_err(|e| anyhow::anyhow!("DEFAULT_LOCALE: {e}"))
                })
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

fn strip_trailing_slash(url: &str) -> String {
    url.trim_end_matches('/').to_owned()
}

fn parse_categories(raw: Option<&str>) -> Result<Vec<u32>> {
    let Some(raw) = raw else {
        return Ok(vec![DEFAULT_CATEGORY]);
    };
    let parsed: Vec<u32> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<u32>()
                .with_context(|| format!("invalid category: {s}"))
        })
        .collect::<Result<_>>()?;

    if parsed.is_empty() {
        bail!("SEARCH_CATEGORIES is set but lists no category");
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn minimal() -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("DISCORD_TOKEN", "token"),
            ("PROWLARR_URL", "http://prowlarr:9696"),
            ("PROWLARR_API_KEY", "key"),
            ("QBIT_URL", "http://qbit:8081"),
        ])
    }

    fn build(vars: &HashMap<&'static str, &'static str>) -> Result<Config> {
        Config::from_lookup(|key| vars.get(key).map(|v| (*v).to_owned()))
    }

    #[test]
    fn defaults_cover_everything_optional() {
        let config = build(&minimal()).unwrap();

        assert_eq!(config.qbit_category, "ebooks");
        assert_eq!(config.search_categories, vec![7020]);
        assert_eq!(config.max_results, 25);
        assert_eq!(config.default_locale, Lang::En);
        assert_eq!(config.guild_id, None);
        assert_eq!(config.qbit_user, None);
    }

    #[test]
    fn every_required_variable_is_enforced() {
        for key in [
            "DISCORD_TOKEN",
            "PROWLARR_URL",
            "PROWLARR_API_KEY",
            "QBIT_URL",
        ] {
            let mut vars = minimal();
            vars.remove(key);
            let error = build(&vars).unwrap_err().to_string();
            assert!(
                error.contains(key),
                "{key} should be reported, got: {error}"
            );
        }
    }

    #[test]
    fn blank_values_count_as_missing() {
        let mut vars = minimal();
        vars.insert("DISCORD_TOKEN", "   ");
        assert!(
            build(&vars)
                .unwrap_err()
                .to_string()
                .contains("DISCORD_TOKEN")
        );
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        let mut vars = minimal();
        vars.insert("PROWLARR_API_KEY", "  key  ");
        assert_eq!(build(&vars).unwrap().prowlarr_api_key, "key");
    }

    #[test]
    fn trailing_slashes_are_stripped_so_paths_do_not_double_up() {
        let mut vars = minimal();
        vars.insert("PROWLARR_URL", "http://prowlarr:9696/");
        vars.insert("QBIT_URL", "http://qbit:8081///");

        let config = build(&vars).unwrap();
        assert_eq!(config.prowlarr_url, "http://prowlarr:9696");
        assert_eq!(config.qbit_url, "http://qbit:8081");
    }

    #[test]
    fn guild_id_must_be_numeric() {
        let mut vars = minimal();
        vars.insert("DISCORD_GUILD_ID", "not-an-id");
        assert!(build(&vars).unwrap_err().to_string().contains("numeric id"));

        vars.insert("DISCORD_GUILD_ID", "1234567890");
        assert_eq!(build(&vars).unwrap().guild_id, Some(1_234_567_890));
    }

    #[test]
    fn categories_parse_from_a_comma_separated_list() {
        let mut vars = minimal();
        vars.insert("SEARCH_CATEGORIES", "7020, 7040 ,3030");
        assert_eq!(
            build(&vars).unwrap().search_categories,
            vec![7020, 7040, 3030]
        );
    }

    #[test]
    fn categories_reject_a_non_numeric_entry() {
        let mut vars = minimal();
        vars.insert("SEARCH_CATEGORIES", "7020,ebook");
        assert!(
            build(&vars)
                .unwrap_err()
                .to_string()
                .contains("invalid category: ebook")
        );
    }

    #[test]
    fn categories_reject_a_list_of_separators_only() {
        let mut vars = minimal();
        vars.insert("SEARCH_CATEGORIES", " , , ");
        assert!(
            build(&vars)
                .unwrap_err()
                .to_string()
                .contains("lists no category")
        );
    }

    #[test]
    fn max_results_must_be_an_integer() {
        let mut vars = minimal();
        vars.insert("MAX_RESULTS", "many");
        assert!(
            build(&vars)
                .unwrap_err()
                .to_string()
                .contains("must be an integer")
        );
    }

    #[test]
    fn max_results_is_bounded_by_the_discord_menu_limit() {
        let mut vars = minimal();
        for invalid in ["0", "26"] {
            vars.insert("MAX_RESULTS", invalid);
            assert!(
                build(&vars)
                    .unwrap_err()
                    .to_string()
                    .contains("between 1 and 25")
            );
        }
        vars.insert("MAX_RESULTS", "10");
        assert_eq!(build(&vars).unwrap().max_results, 10);
    }

    #[test]
    fn the_default_locale_can_be_switched_to_french() {
        let mut vars = minimal();
        vars.insert("DEFAULT_LOCALE", "fr");
        assert_eq!(build(&vars).unwrap().default_locale, Lang::Fr);
    }

    #[test]
    fn an_unsupported_default_locale_is_rejected_by_name() {
        let mut vars = minimal();
        vars.insert("DEFAULT_LOCALE", "de");
        let error = build(&vars).unwrap_err().to_string();
        assert!(error.contains("DEFAULT_LOCALE"), "got: {error}");
        assert!(error.contains("de"), "got: {error}");
    }

    #[test]
    fn from_env_delegates_to_the_process_environment() {
        // The parsing itself is covered above; this pins the delegation so the
        // two entry points cannot drift apart.
        let direct = Config::from_lookup(|key| std::env::var(key).ok());
        assert_eq!(direct.is_ok(), Config::from_env().is_ok());
    }

    #[test]
    fn qbittorrent_credentials_are_optional_but_read_when_present() {
        let mut vars = minimal();
        vars.insert("QBIT_USER", "admin");
        vars.insert("QBIT_PASS", "secret");

        let config = build(&vars).unwrap();
        assert_eq!(config.qbit_user.as_deref(), Some("admin"));
        assert_eq!(config.qbit_pass.as_deref(), Some("secret"));
    }
}
