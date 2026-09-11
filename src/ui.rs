//! Pure builders. Keeping the decisions here is what makes them testable
//! without a Discord connection.

use crate::i18n::Lang;
use crate::prowlarr::Release;
use crate::qbittorrent::Category;
use crate::watchlist::{SECONDS_PER_DAY, Watch};
use humansize::{DECIMAL, format_size};
use poise::serenity_prelude as serenity;
use std::collections::BTreeMap;

const BLUE: u32 = 0x3498db;
const GREEN: u32 = 0x2ecc71;
const RED: u32 = 0xe74c3c;
const GREY: u32 = 0x95a5a6;

/// Listed in the embed body; the rest stay reachable from the menu.
const LISTED: usize = 10;
/// Discord rejects longer menu labels and descriptions.
const LABEL_MAX: usize = 100;

/// On a character boundary: byte slicing panics on the accented titles.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    text.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

pub fn results_embed(query: &str, results: &[Release], lang: Lang) -> serenity::CreateEmbed {
    let lines = results
        .iter()
        .take(LISTED)
        .enumerate()
        .map(|(i, r)| {
            format!(
                "`{:>2}` **{}**\n     {} · 🌱 {} · {}",
                i + 1,
                truncate(&r.title, 80),
                format_size(r.size, DECIMAL),
                r.seeders.unwrap_or(0),
                r.indexer
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let hidden = results.len().saturating_sub(LISTED);
    let footer = if hidden > 0 {
        lang.more_in_menu(hidden)
    } else {
        String::new()
    };

    serenity::CreateEmbed::new()
        .title(format!("📚 {query}"))
        .description(format!("{lines}{footer}"))
        .colour(BLUE)
}

pub fn expired_embed(query: &str, results: &[Release], lang: Lang) -> serenity::CreateEmbed {
    results_embed(query, results, lang).colour(GREY)
}

pub fn select_options(results: &[Release], lang: Lang) -> Vec<serenity::CreateSelectMenuOption> {
    results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let summary = lang.release_summary(
                &format_size(r.size, DECIMAL),
                r.seeders.unwrap_or(0),
                &r.indexer,
            );
            serenity::CreateSelectMenuOption::new(truncate(&r.title, LABEL_MAX), i.to_string())
                .description(truncate(&summary, LABEL_MAX))
        })
        .collect()
}

/// Values come from Discord, so an unusable index is expected, not an invariant.
pub fn parse_selection<'a>(values: &[String], results: &'a [Release]) -> Option<&'a Release> {
    values
        .first()?
        .parse::<usize>()
        .ok()
        .and_then(|i| results.get(i))
}

pub fn added_embed(
    release: &Release,
    category: &str,
    save_path: &str,
    lang: Lang,
) -> serenity::CreateEmbed {
    serenity::CreateEmbed::new()
        .title(lang.added_title())
        .description(format!("**{}**", release.title))
        .field(lang.field_category(), format!("`{category}`"), true)
        .field(
            lang.field_destination(),
            if save_path.is_empty() {
                lang.default_folder().to_owned()
            } else {
                format!("`{save_path}`")
            },
            true,
        )
        .field(lang.field_size(), format_size(release.size, DECIMAL), true)
        .colour(GREEN)
}

pub fn failed_embed(release: &Release, error: &str, lang: Lang) -> serenity::CreateEmbed {
    serenity::CreateEmbed::new()
        .title(lang.failed_title())
        .description(format!("**{}**\n```{error}```", release.title))
        .colour(RED)
}

/// Flags the two cases that silently write to the wrong place.
pub fn destination_label(
    categories: &BTreeMap<String, Category>,
    category: &str,
    lang: Lang,
) -> String {
    match categories.get(category) {
        Some(c) if !c.save_path.is_empty() => format!("`{}`", c.save_path),
        Some(_) => lang.no_save_path().to_owned(),
        None => lang.category_missing(category),
    }
}

/// Offered when a search comes back empty, so the user can leave it running.
pub fn watch_button(custom_id: &str, lang: Lang) -> serenity::CreateActionRow {
    serenity::CreateActionRow::Buttons(vec![
        serenity::CreateButton::new(custom_id)
            .style(serenity::ButtonStyle::Primary)
            .emoji('🔔')
            .label(lang.watch_button()),
    ])
}

/// Rounded up, so a watch with a few hours left still reads as one day rather
/// than as zero.
pub fn days_left(watch: &Watch, now: i64, max_age: i64) -> i64 {
    // i64::div_ceil is still unstable; remaining is never negative here.
    let remaining = (watch.created_at + max_age - now).max(0);
    (remaining + SECONDS_PER_DAY - 1) / SECONDS_PER_DAY
}

pub fn watchlist_embed(
    watches: &[Watch],
    now: i64,
    max_age: i64,
    lang: Lang,
) -> serenity::CreateEmbed {
    let embed = serenity::CreateEmbed::new()
        .title(lang.watchlist_title())
        .colour(BLUE);
    if watches.is_empty() {
        return embed.description(lang.watchlist_empty()).colour(GREY);
    }

    embed.description(
        watches
            .iter()
            .map(|w| {
                let state = if w.notified_at.is_some() {
                    format!("🔔 {}", lang.watchlist_waiting())
                } else {
                    lang.watchlist_entry(w.checks, days_left(w, now, max_age))
                };
                format!("**{}**\n     {state}", truncate(&w.query, 80))
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub fn watchlist_options(watches: &[Watch], lang: Lang) -> Vec<serenity::CreateSelectMenuOption> {
    watches
        .iter()
        .map(|w| {
            serenity::CreateSelectMenuOption::new(truncate(&w.query, LABEL_MAX), w.id.to_string())
                .description(truncate(&lang.watchlist_entry(w.checks, 0), LABEL_MAX))
        })
        .collect()
}

pub fn status_embed(
    prowlarr: &anyhow::Result<String>,
    qbit: &anyhow::Result<String>,
    category: &str,
    destination: &str,
    lang: Lang,
) -> serenity::CreateEmbed {
    let line = |label: &str, res: &anyhow::Result<String>| match res {
        Ok(v) => format!("✅ **{label}** — {}", v.trim()),
        Err(e) => format!("❌ **{label}** — {e}"),
    };

    serenity::CreateEmbed::new()
        .title(lang.status_title())
        .description(format!(
            "{}\n{}",
            line("Prowlarr", prowlarr),
            line("qBittorrent", qbit)
        ))
        .field(lang.field_category(), format!("`{category}`"), true)
        .field(lang.field_destination(), destination, true)
        .colour(if prowlarr.is_ok() && qbit.is_ok() {
            GREEN
        } else {
            RED
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const BOTH: [Lang; 2] = [Lang::En, Lang::Fr];

    fn release(title: &str, size: u64, seeders: Option<u32>) -> Release {
        Release {
            title: title.to_owned(),
            guid: format!("guid-{title}"),
            indexer_id: 1,
            indexer: "TestIndexer".to_owned(),
            size,
            seeders,
            leechers: Some(1),
            download_url: Some("http://example.test/file.torrent".to_owned()),
            magnet_url: None,
            protocol: "torrent".to_owned(),
        }
    }

    fn json(embed: serenity::CreateEmbed) -> Value {
        serde_json::to_value(embed).expect("an embed serializes")
    }

    #[test]
    fn truncate_keeps_short_text_untouched() {
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn truncate_never_splits_a_multibyte_character() {
        assert_eq!(truncate("éééééé", 3), "éé…");
        assert_eq!(truncate("éééééé", 6), "éééééé");
        assert_eq!(truncate("abc", 1), "…");
    }

    #[test]
    fn results_embed_lists_entries_with_size_and_seeders() {
        let results = [release("Dune", 930_000, Some(40))];
        for lang in BOTH {
            let value = json(results_embed("dune", &results, lang));
            let description = value["description"].as_str().unwrap();

            assert_eq!(value["title"], "📚 dune");
            assert!(description.contains("Dune"));
            assert!(description.contains("930 kB"), "got: {description}");
            assert!(description.contains("40"));
            assert_eq!(value["color"], BLUE);
        }
    }

    #[test]
    fn results_embed_announces_the_overflow_in_each_language() {
        let results: Vec<Release> = (0..13)
            .map(|i| release(&format!("Book {i}"), 1000, Some(1)))
            .collect();

        let english = json(results_embed("books", &results, Lang::En));
        assert!(
            english["description"]
                .as_str()
                .unwrap()
                .contains("and 3 more")
        );

        let french = json(results_embed("books", &results, Lang::Fr));
        let description = french["description"].as_str().unwrap();
        assert!(description.contains("et 3 autre"), "got: {description}");
        assert!(!description.contains("Book 12"));
    }

    #[test]
    fn results_embed_stays_quiet_when_nothing_overflows() {
        let results = [release("Only one", 1000, Some(1))];
        for lang in BOTH {
            let value = json(results_embed("q", &results, lang));
            let description = value["description"].as_str().unwrap();
            assert!(!description.contains("menu"), "got: {description}");
        }
    }

    #[test]
    fn missing_seeder_counts_render_as_zero() {
        let results = [release("Usenet release", 1000, None)];
        let value = json(results_embed("q", &results, Lang::En));
        assert!(value["description"].as_str().unwrap().contains("🌱 0"));
    }

    #[test]
    fn expired_embed_only_changes_the_colour() {
        let results = [release("Dune", 1000, Some(1))];
        for lang in BOTH {
            let fresh = json(results_embed("q", &results, lang));
            let expired = json(expired_embed("q", &results, lang));

            assert_eq!(fresh["description"], expired["description"]);
            assert_eq!(expired["color"], GREY);
        }
    }

    #[test]
    fn select_options_index_every_result() {
        let results: Vec<Release> = (0..3)
            .map(|i| release(&format!("Book {i}"), 2048, Some(7)))
            .collect();
        let value = serde_json::to_value(select_options(&results, Lang::En)).unwrap();

        assert_eq!(value.as_array().unwrap().len(), 3);
        assert_eq!(value[0]["value"], "0");
        assert_eq!(value[2]["value"], "2");
        assert!(
            value[1]["description"]
                .as_str()
                .unwrap()
                .contains("7 seeders")
        );
    }

    #[test]
    fn select_option_summaries_are_translated() {
        let results = [release("Dune", 2048, Some(7))];
        let value = serde_json::to_value(select_options(&results, Lang::Fr)).unwrap();
        assert!(
            value[0]["description"]
                .as_str()
                .unwrap()
                .contains("7 sources")
        );
    }

    #[test]
    fn select_options_respect_the_discord_length_limit() {
        let results = [release(&"z".repeat(250), 1000, Some(1))];
        for lang in BOTH {
            let value = serde_json::to_value(select_options(&results, lang)).unwrap();
            let label = value[0]["label"].as_str().unwrap();

            assert_eq!(label.chars().count(), LABEL_MAX);
            assert!(label.ends_with('…'));
        }
    }

    #[test]
    fn parse_selection_resolves_the_picked_index() {
        let results = [release("first", 1, Some(1)), release("second", 1, Some(1))];
        assert_eq!(
            parse_selection(&["1".to_owned()], &results).unwrap().title,
            "second"
        );
    }

    #[test]
    fn parse_selection_rejects_anything_unexpected() {
        let results = [release("first", 1, Some(1))];
        assert!(parse_selection(&[], &results).is_none());
        assert!(parse_selection(&["not a number".to_owned()], &results).is_none());
        assert!(parse_selection(&["9".to_owned()], &results).is_none());
    }

    #[test]
    fn added_embed_reports_category_and_destination() {
        let value = json(added_embed(
            &release("Dune", 930_000, Some(40)),
            "ebooks",
            "/data/ebooks",
            Lang::En,
        ));

        assert_eq!(value["color"], GREEN);
        assert_eq!(value["title"], "📥 Sent to qBittorrent");
        assert_eq!(value["fields"][0]["name"], "Category");
        assert_eq!(value["fields"][0]["value"], "`ebooks`");
        assert_eq!(value["fields"][1]["value"], "`/data/ebooks`");
    }

    #[test]
    fn added_embed_is_translated() {
        let value = json(added_embed(
            &release("Dune", 930_000, Some(40)),
            "ebooks",
            "/data/ebooks",
            Lang::Fr,
        ));

        assert_eq!(value["title"], "📥 Envoyé à qBittorrent");
        assert_eq!(value["fields"][0]["name"], "Catégorie");
        assert_eq!(value["fields"][2]["name"], "Taille");
    }

    #[test]
    fn added_embed_says_so_when_no_path_was_resolved() {
        assert_eq!(
            json(added_embed(
                &release("Dune", 1000, Some(1)),
                "ebooks",
                "",
                Lang::En
            ))["fields"][1]["value"],
            "qBittorrent default folder"
        );
        assert_eq!(
            json(added_embed(
                &release("Dune", 1000, Some(1)),
                "ebooks",
                "",
                Lang::Fr
            ))["fields"][1]["value"],
            "dossier par défaut de qBittorrent"
        );
    }

    #[test]
    fn failed_embed_surfaces_the_error_in_each_language() {
        for lang in BOTH {
            let value = json(failed_embed(
                &release("Dune", 1000, Some(1)),
                "connection refused",
                lang,
            ));

            assert_eq!(value["color"], RED);
            assert_eq!(value["title"], lang.failed_title());
            assert!(
                value["description"]
                    .as_str()
                    .unwrap()
                    .contains("connection refused")
            );
        }
    }

    #[test]
    fn destination_label_reports_a_configured_path_regardless_of_language() {
        let cats = BTreeMap::from([(
            "ebooks".to_owned(),
            Category {
                name: "ebooks".to_owned(),
                save_path: "/data/ebooks".to_owned(),
            },
        )]);
        for lang in BOTH {
            assert_eq!(destination_label(&cats, "ebooks", lang), "`/data/ebooks`");
        }
    }

    #[test]
    fn destination_label_warns_when_the_category_has_no_path() {
        let cats = BTreeMap::from([(
            "ebooks".to_owned(),
            Category {
                name: "ebooks".to_owned(),
                save_path: String::new(),
            },
        )]);
        assert!(destination_label(&cats, "ebooks", Lang::En).contains("no save path"));
        assert!(destination_label(&cats, "ebooks", Lang::Fr).contains("aucun chemin"));
    }

    #[test]
    fn destination_label_warns_when_the_category_is_missing() {
        assert!(destination_label(&BTreeMap::new(), "ebooks", Lang::En).contains("does not exist"));
        assert!(destination_label(&BTreeMap::new(), "ebooks", Lang::Fr).contains("absente"));
    }

    fn watch(query: &str, created_at: i64, checks: u32) -> Watch {
        Watch {
            id: 1,
            user_id: 42,
            channel_id: 100,
            query: query.to_owned(),
            created_at,
            last_checked_at: created_at,
            checks,
            lang: Lang::En,
            notified_at: None,
        }
    }

    #[test]
    fn the_watch_button_carries_the_custom_id_and_is_translated() {
        let value = serde_json::to_value(watch_button("watch:7", Lang::Fr)).unwrap();
        let button = &value["components"][0];

        assert_eq!(button["custom_id"], "watch:7");
        assert_eq!(button["label"], Lang::Fr.watch_button());
    }

    #[test]
    fn days_left_rounds_up_so_it_never_reads_as_zero_too_early() {
        let w = watch("dune", 0, 0);
        let max_age = 30 * SECONDS_PER_DAY;

        assert_eq!(days_left(&w, 0, max_age), 30);
        assert_eq!(days_left(&w, 29 * SECONDS_PER_DAY, max_age), 1);
        // Still twelve hours to go: that is one day, not none.
        assert_eq!(days_left(&w, 30 * SECONDS_PER_DAY - 43_200, max_age), 1);
    }

    #[test]
    fn days_left_never_goes_negative() {
        let w = watch("dune", 0, 0);
        assert_eq!(
            days_left(&w, 999 * SECONDS_PER_DAY, 30 * SECONDS_PER_DAY),
            0
        );
    }

    #[test]
    fn an_empty_watchlist_explains_how_to_create_one() {
        let value = json(watchlist_embed(&[], 0, SECONDS_PER_DAY, Lang::En));

        assert_eq!(value["color"], GREY);
        assert!(
            value["description"]
                .as_str()
                .unwrap()
                .contains("no standing search")
        );
    }

    #[test]
    fn a_watchlist_lists_each_query_with_its_progress() {
        let watches = [watch("dune", 0, 3), watch("hyperion", 0, 1)];
        let value = json(watchlist_embed(&watches, 0, 30 * SECONDS_PER_DAY, Lang::En));
        let description = value["description"].as_str().unwrap();

        assert_eq!(value["color"], BLUE);
        assert!(description.contains("dune"));
        assert!(description.contains("hyperion"));
        assert!(
            description.contains("checked 3 times"),
            "got: {description}"
        );
        assert!(description.contains("30 days left"));
    }

    #[test]
    fn a_found_watch_is_marked_as_waiting_for_the_download() {
        let mut found = watch("dune", 0, 3);
        found.notified_at = Some(100);
        let value = json(watchlist_embed(&[found], 0, 30 * SECONDS_PER_DAY, Lang::En));
        let description = value["description"].as_str().unwrap();

        assert!(
            description.contains("waiting for your download"),
            "got: {description}"
        );
        assert!(!description.contains("checked 3 times"));
    }

    #[test]
    fn the_watchlist_is_translated() {
        let watches = [watch("dune", 0, 3)];
        let value = json(watchlist_embed(&watches, 0, 30 * SECONDS_PER_DAY, Lang::Fr));

        assert_eq!(value["title"], Lang::Fr.watchlist_title());
        assert!(
            value["description"]
                .as_str()
                .unwrap()
                .contains("vérifiée 3 fois")
        );
    }

    #[test]
    fn watchlist_options_are_keyed_by_watch_id() {
        let mut second = watch("hyperion", 0, 0);
        second.id = 9;
        let value =
            serde_json::to_value(watchlist_options(&[watch("dune", 0, 0), second], Lang::En))
                .unwrap();

        assert_eq!(value[0]["value"], "1");
        assert_eq!(value[1]["value"], "9");
        assert_eq!(value[1]["label"], "hyperion");
    }

    #[test]
    fn watchlist_options_respect_the_discord_length_limit() {
        let value = serde_json::to_value(watchlist_options(
            &[watch(&"z".repeat(250), 0, 0)],
            Lang::En,
        ))
        .unwrap();
        assert_eq!(
            value[0]["label"].as_str().unwrap().chars().count(),
            LABEL_MAX
        );
    }

    #[test]
    fn status_embed_is_green_only_when_both_services_answer() {
        let value = json(status_embed(
            &Ok("2.5.2".to_owned()),
            &Ok("v5.2.3".to_owned()),
            "ebooks",
            "`/data/ebooks`",
            Lang::En,
        ));

        assert_eq!(value["color"], GREEN);
        assert_eq!(value["title"], "Service status");
        assert!(value["description"].as_str().unwrap().contains("2.5.2"));
    }

    #[test]
    fn status_embed_is_translated() {
        let value = json(status_embed(
            &Ok("2.5.2".to_owned()),
            &Ok("v5.2.3".to_owned()),
            "ebooks",
            "`/data/ebooks`",
            Lang::Fr,
        ));
        assert_eq!(value["title"], "État des services");
    }

    #[test]
    fn status_embed_turns_red_and_shows_why() {
        let value = json(status_embed(
            &Ok("2.5.2".to_owned()),
            &Err(anyhow::anyhow!("qBittorrent unreachable")),
            "ebooks",
            "`/data/ebooks`",
            Lang::En,
        ));

        assert_eq!(value["color"], RED);
        assert!(
            value["description"]
                .as_str()
                .unwrap()
                .contains("qBittorrent unreachable")
        );
    }
}
