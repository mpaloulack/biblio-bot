use crate::i18n::Lang;
use crate::watchlist::SECONDS_PER_DAY;
use anyhow::{Context as _, Result, bail};
use std::env;
use std::path::PathBuf;

const DISCORD_MENU_MAX: usize = 25;
const BOOKS_EBOOK: u32 = 7020;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub discord_token: String,
    /// Set to register commands instantly in one guild instead of globally.
    pub guild_id: Option<u64>,
    pub prowlarr_url: String,
    pub prowlarr_api_key: String,
    pub qbit_url: String,
    pub qbit_user: Option<String>,
    pub qbit_pass: Option<String>,
    pub qbit_category: String,
    /// Newznab category ids.
    pub search_categories: Vec<u32>,
    pub max_results: usize,
    /// Used for locales we do not translate.
    pub default_locale: Lang,
    /// How often a standing search is retried, per day.
    pub watch_checks_per_day: u32,
    /// A standing search gives up after this many days.
    pub watch_max_days: i64,
    pub watch_max_per_user: usize,
    pub watchlist_path: PathBuf,
    /// How often a tracked download is checked for having finished seeding.
    pub download_check_interval_secs: i64,
    pub downloads_path: PathBuf,
}

impl Config {
    /// What the bot is actually configured to talk to, for the startup log.
    ///
    /// Secrets are reported as present or absent, never echoed: this line ends
    /// up in container logs, which people paste into issues.
    pub fn summary(&self) -> String {
        format!(
            "prowlarr={} qbit={} qbit_auth={} category={} categories={:?} \
             locale={} guild={} max_results={} watch={}/day max_days={} \
             max_per_user={} watchlist={} download_check={}s downloads={}",
            self.prowlarr_url,
            self.qbit_url,
            if self.qbit_user.is_some() && self.qbit_pass.is_some() {
                "credentials"
            } else {
                "none (relying on qBittorrent's subnet bypass)"
            },
            self.qbit_category,
            self.search_categories,
            self.default_locale,
            self.guild_id
                .map_or_else(|| "global".to_owned(), |id| id.to_string()),
            self.max_results,
            self.watch_checks_per_day,
            self.watch_max_days,
            self.watch_max_per_user,
            self.watchlist_path.display(),
            self.download_check_interval_secs,
            self.downloads_path.display(),
        )
    }

    pub fn watch_interval_secs(&self) -> i64 {
        SECONDS_PER_DAY / i64::from(self.watch_checks_per_day)
    }

    pub fn watch_max_age_secs(&self) -> i64 {
        self.watch_max_days * SECONDS_PER_DAY
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// Tests pass their own source rather than racing on the process environment.
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
            .unwrap_or(DISCORD_MENU_MAX);
        if !(1..=DISCORD_MENU_MAX).contains(&max_results) {
            bail!(
                "MAX_RESULTS must be between 1 and {DISCORD_MENU_MAX} (Discord select menu limit)"
            );
        }

        let bounded = |key: &str, default: u64, min: u64, max: u64| -> Result<u64> {
            let value = get(key)
                .map(|v| {
                    v.parse::<u64>()
                        .with_context(|| format!("{key} must be an integer"))
                })
                .transpose()?
                .unwrap_or(default);
            if !(min..=max).contains(&value) {
                bail!("{key} must be between {min} and {max}");
            }
            Ok(value)
        };

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
            // A day has to divide into whole intervals, so cap at 24.
            watch_checks_per_day: bounded("WATCH_CHECKS_PER_DAY", 4, 1, 24)? as u32,
            watch_max_days: bounded("WATCH_MAX_DAYS", 30, 1, 365)? as i64,
            watch_max_per_user: bounded("WATCH_MAX_PER_USER", 10, 1, 100)? as usize,
            watchlist_path: get("WATCHLIST_PATH")
                .unwrap_or_else(|| "data/watchlist.json".to_owned())
                .into(),
            download_check_interval_secs: bounded("DOWNLOAD_CHECK_INTERVAL_SECS", 300, 60, 3_600)?
                as i64,
            downloads_path: get("DOWNLOADS_PATH")
                .unwrap_or_else(|| "data/downloads.json".to_owned())
                .into(),
        })
    }
}

fn strip_trailing_slash(url: &str) -> String {
    url.trim_end_matches('/').to_owned()
}

fn parse_categories(raw: Option<&str>) -> Result<Vec<u32>> {
    let Some(raw) = raw else {
        return Ok(vec![BOOKS_EBOOK]);
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
        assert_eq!(config.watch_checks_per_day, 4);
        assert_eq!(config.watch_max_days, 30);
        assert_eq!(config.watch_max_per_user, 10);
        assert_eq!(config.watchlist_path, PathBuf::from("data/watchlist.json"));
        assert_eq!(config.guild_id, None);
        assert_eq!(config.qbit_user, None);
        assert_eq!(config.download_check_interval_secs, 300);
        assert_eq!(config.downloads_path, PathBuf::from("data/downloads.json"));
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
        let direct = Config::from_lookup(|key| std::env::var(key).ok());
        assert_eq!(direct.is_ok(), Config::from_env().is_ok());
    }

    #[test]
    fn the_summary_reports_what_the_bot_will_talk_to() {
        let summary = build(&minimal()).unwrap().summary();

        assert!(
            summary.contains("prowlarr=http://prowlarr:9696"),
            "got: {summary}"
        );
        assert!(summary.contains("qbit=http://qbit:8081"));
        assert!(summary.contains("category=ebooks"));
        assert!(summary.contains("watch=4/day"));
        assert!(summary.contains("guild=global"));
        assert!(summary.contains("download_check=300s"));
        assert!(summary.contains("downloads=data/downloads.json"));
    }

    #[test]
    fn the_summary_never_echoes_a_secret() {
        let mut vars = minimal();
        vars.insert("DISCORD_TOKEN", "TOKEN-must-not-leak");
        vars.insert("PROWLARR_API_KEY", "APIKEY-must-not-leak");
        vars.insert("QBIT_USER", "admin");
        vars.insert("QBIT_PASS", "PASSWORD-must-not-leak");

        let summary = build(&vars).unwrap().summary();
        for secret in [
            "TOKEN-must-not-leak",
            "APIKEY-must-not-leak",
            "PASSWORD-must-not-leak",
        ] {
            assert!(!summary.contains(secret), "{secret} leaked into: {summary}");
        }
        assert!(summary.contains("qbit_auth=credentials"));
    }

    #[test]
    fn the_summary_says_when_no_credentials_are_set() {
        assert!(
            build(&minimal())
                .unwrap()
                .summary()
                .contains("qbit_auth=none")
        );
    }

    #[test]
    fn the_summary_names_the_guild_when_scoped() {
        let mut vars = minimal();
        vars.insert("DISCORD_GUILD_ID", "655831756498403334");
        assert!(
            build(&vars)
                .unwrap()
                .summary()
                .contains("guild=655831756498403334")
        );
    }

    #[test]
    fn the_check_interval_divides_the_day() {
        let mut vars = minimal();
        vars.insert("WATCH_CHECKS_PER_DAY", "4");
        assert_eq!(build(&vars).unwrap().watch_interval_secs(), 6 * 3_600);

        vars.insert("WATCH_CHECKS_PER_DAY", "24");
        assert_eq!(build(&vars).unwrap().watch_interval_secs(), 3_600);

        vars.insert("WATCH_CHECKS_PER_DAY", "1");
        assert_eq!(build(&vars).unwrap().watch_interval_secs(), 86_400);
    }

    #[test]
    fn the_maximum_age_is_expressed_in_seconds() {
        let mut vars = minimal();
        vars.insert("WATCH_MAX_DAYS", "7");
        assert_eq!(build(&vars).unwrap().watch_max_age_secs(), 7 * 86_400);
    }

    #[test]
    fn watch_settings_are_bounded() {
        let cases = [
            ("WATCH_CHECKS_PER_DAY", ["0", "25"]),
            ("WATCH_MAX_DAYS", ["0", "366"]),
            ("WATCH_MAX_PER_USER", ["0", "101"]),
            ("DOWNLOAD_CHECK_INTERVAL_SECS", ["59", "3601"]),
        ];
        for (key, invalid) in cases {
            for value in invalid {
                let mut vars = minimal();
                vars.insert(key, value);
                let error = build(&vars).unwrap_err().to_string();
                assert!(
                    error.contains(key),
                    "{key}={value} should be rejected, got: {error}"
                );
            }
        }
    }

    #[test]
    fn a_non_numeric_watch_setting_is_rejected() {
        let mut vars = minimal();
        vars.insert("WATCH_CHECKS_PER_DAY", "often");
        assert!(
            build(&vars)
                .unwrap_err()
                .to_string()
                .contains("must be an integer")
        );
    }

    #[test]
    fn the_watchlist_path_can_be_moved() {
        let mut vars = minimal();
        vars.insert("WATCHLIST_PATH", "/var/lib/biblio/watches.json");
        assert_eq!(
            build(&vars).unwrap().watchlist_path,
            PathBuf::from("/var/lib/biblio/watches.json")
        );
    }

    #[test]
    fn the_downloads_path_can_be_moved() {
        let mut vars = minimal();
        vars.insert("DOWNLOADS_PATH", "/var/lib/biblio/downloads.json");
        assert_eq!(
            build(&vars).unwrap().downloads_path,
            PathBuf::from("/var/lib/biblio/downloads.json")
        );
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
