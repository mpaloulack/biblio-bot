//! Standing searches: what to re-run later, and when.
//!
//! Kept as a plain JSON file rather than a database — it holds a handful of
//! rows, and being readable from the host is worth more here than queries.

use crate::i18n::Lang;
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

pub const SECONDS_PER_DAY: i64 = 86_400;

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Watch {
    pub id: u64,
    pub user_id: u64,
    pub channel_id: u64,
    /// Scopes the moderator view: an admin of one server has no business
    /// seeing, or stopping, what someone set up in another.
    #[serde(default)]
    pub guild_id: Option<u64>,
    pub query: String,
    pub created_at: i64,
    pub last_checked_at: i64,
    pub checks: u32,
    /// The language the notification should use; the user's Discord locale is
    /// not reachable from the background task.
    #[serde(default)]
    pub lang: Lang,
    /// Set once the user has been told the book showed up. The watch then stops
    /// searching but stays listed until the download actually happens.
    #[serde(default)]
    pub notified_at: Option<i64>,
}

impl Watch {
    pub fn is_due(&self, now: i64, interval: i64) -> bool {
        self.notified_at.is_none() && now - self.last_checked_at >= interval
    }

    pub fn is_expired(&self, now: i64, max_age: i64) -> bool {
        now - self.created_at >= max_age
    }
}

/// What a caller supplies to open a watch; the rest is bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWatch {
    pub user_id: u64,
    pub channel_id: u64,
    pub guild_id: Option<u64>,
    pub query: String,
    pub lang: Lang,
}

/// Why a watch could not be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectedWatch {
    AlreadyWatching,
    TooMany,
}

/// The whole file, and every decision taken on it. Synchronous and pure, so the
/// scheduling rules can be tested without a runtime or a filesystem.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    next_id: u64,
    watches: Vec<Watch>,
}

impl State {
    pub fn add(
        &mut self,
        new: NewWatch,
        now: i64,
        max_per_user: usize,
    ) -> Result<&Watch, RejectedWatch> {
        let mine = |w: &&Watch| w.user_id == new.user_id;
        if self
            .watches
            .iter()
            .filter(mine)
            .any(|w| w.query == new.query)
        {
            return Err(RejectedWatch::AlreadyWatching);
        }
        if self.watches.iter().filter(mine).count() >= max_per_user {
            return Err(RejectedWatch::TooMany);
        }

        self.next_id += 1;
        self.watches.push(Watch {
            id: self.next_id,
            user_id: new.user_id,
            channel_id: new.channel_id,
            guild_id: new.guild_id,
            query: new.query,
            lang: new.lang,
            notified_at: None,
            created_at: now,
            // Not due until a full interval has passed: the search just ran.
            last_checked_at: now,
            checks: 0,
        });
        Ok(self.watches.last().expect("just pushed"))
    }

    /// Removes a watch, but only from the user who created it.
    pub fn remove(&mut self, id: u64, user_id: u64) -> Option<Watch> {
        let index = self
            .watches
            .iter()
            .position(|w| w.id == id && w.user_id == user_id)?;
        Some(self.watches.remove(index))
    }

    pub fn for_user(&self, user_id: u64) -> Vec<&Watch> {
        self.watches
            .iter()
            .filter(|w| w.user_id == user_id)
            .collect()
    }

    /// Everything running in one server, for the moderator view.
    pub fn for_guild(&self, guild_id: u64) -> Vec<&Watch> {
        self.watches
            .iter()
            .filter(|w| w.guild_id == Some(guild_id))
            .collect()
    }

    /// Removes without checking ownership. Callers must have established that
    /// the requester moderates the server the watch belongs to.
    pub fn remove_within_guild(&mut self, id: u64, guild_id: u64) -> Option<Watch> {
        let index = self
            .watches
            .iter()
            .position(|w| w.id == id && w.guild_id == Some(guild_id))?;
        Some(self.watches.remove(index))
    }

    pub fn due(&self, now: i64, interval: i64) -> Vec<Watch> {
        self.watches
            .iter()
            .filter(|w| w.is_due(now, interval))
            .cloned()
            .collect()
    }

    /// Records that the user has been told, which stops the searching.
    pub fn mark_notified(&mut self, id: u64, now: i64) {
        if let Some(watch) = self.watches.iter_mut().find(|w| w.id == id) {
            watch.notified_at = Some(now);
        }
    }

    /// Drops the watches this user was keeping for that exact search, which is
    /// what a successful download means.
    pub fn fulfil(&mut self, user_id: u64, query: &str) -> Vec<Watch> {
        let matches =
            |w: &Watch| w.user_id == user_id && w.query.trim().eq_ignore_ascii_case(query.trim());
        let (fulfilled, kept) = std::mem::take(&mut self.watches)
            .into_iter()
            .partition(matches);
        self.watches = kept;
        fulfilled
    }

    /// Seconds until the earliest watch is checked again, ignoring the ones
    /// already found and waiting on their owner. `None` when nothing is
    /// searching. Exists so the logs can say why nothing is happening yet.
    pub fn seconds_until_next_due(&self, now: i64, interval: i64) -> Option<i64> {
        self.watches
            .iter()
            .filter(|w| w.notified_at.is_none())
            .map(|w| (w.last_checked_at + interval - now).max(0))
            .min()
    }

    pub fn mark_checked(&mut self, id: u64, now: i64) {
        if let Some(watch) = self.watches.iter_mut().find(|w| w.id == id) {
            watch.last_checked_at = now;
            watch.checks += 1;
        }
    }

    /// Drops watches that have been running for too long, returning them so the
    /// caller can tell their owners why they stopped.
    pub fn prune_expired(&mut self, now: i64, max_age: i64) -> Vec<Watch> {
        let (expired, kept) = std::mem::take(&mut self.watches)
            .into_iter()
            .partition(|w| w.is_expired(now, max_age));
        self.watches = kept;
        expired
    }

    pub fn len(&self) -> usize {
        self.watches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.watches.is_empty()
    }
}

/// The file-backed watchlist shared by the commands and the background task.
#[derive(Debug)]
pub struct Watchlist {
    path: PathBuf,
    state: Mutex<State>,
}

impl Watchlist {
    /// A missing file is an empty watchlist; an unreadable one is an error,
    /// because silently starting empty would throw away someone's watches.
    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let state = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("{} is not a readable watchlist", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    pub async fn with<T>(&self, f: impl FnOnce(&State) -> T) -> T {
        f(&*self.state.lock().await)
    }

    /// Applies a change and persists it. The write is atomic: a crash halfway
    /// through leaves the previous file, not a truncated one.
    pub async fn update<T>(&self, f: impl FnOnce(&mut State) -> T) -> Result<T> {
        let mut state = self.state.lock().await;
        let outcome = f(&mut state);

        let serialized = serde_json::to_vec_pretty(&*state)?;
        if let Some(parent) = self.path.parent() {
            // A bare filename yields an empty parent, which simply fails here.
            tokio::fs::create_dir_all(parent).await.ok();
        }
        let temporary = self.path.with_extension("json.tmp");
        tokio::fs::write(&temporary, &serialized)
            .await
            .with_context(|| format!("cannot write {}", temporary.display()))?;
        tokio::fs::rename(&temporary, &self.path)
            .await
            .with_context(|| format!("cannot replace {}", self.path.display()))?;

        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600;

    fn request(user_id: u64, guild_id: Option<u64>, query: &str, lang: Lang) -> NewWatch {
        NewWatch {
            user_id,
            channel_id: 100,
            guild_id,
            query: query.to_owned(),
            lang,
        }
    }

    fn state_with(entries: &[(u64, &str, i64)]) -> State {
        let mut state = State::default();
        for (user, query, now) in entries {
            state
                .add(request(*user, Some(1), query, Lang::En), *now, 10)
                .expect("accepted");
        }
        state
    }

    #[test]
    fn adding_assigns_increasing_ids() {
        let state = state_with(&[(1, "dune", 0), (1, "neuromancer", 0)]);
        let ids: Vec<u64> = state.for_user(1).iter().map(|w| w.id).collect();
        assert_eq!(ids, [1, 2]);
    }

    #[test]
    fn a_new_watch_is_not_immediately_due() {
        // The search that created it just ran; checking again at once is waste.
        let state = state_with(&[(1, "dune", 1_000)]);
        assert!(state.due(1_000, HOUR).is_empty());
    }

    #[test]
    fn a_watch_becomes_due_after_one_interval() {
        let state = state_with(&[(1, "dune", 1_000)]);
        assert!(state.due(1_000 + HOUR - 1, HOUR).is_empty());
        assert_eq!(state.due(1_000 + HOUR, HOUR).len(), 1);
    }

    #[test]
    fn the_same_query_cannot_be_watched_twice_by_one_user() {
        let mut state = state_with(&[(1, "dune", 0)]);
        assert_eq!(
            state.add(request(1, Some(1), "dune", Lang::En), 0, 10),
            Err(RejectedWatch::AlreadyWatching)
        );
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn two_users_may_watch_the_same_query() {
        let mut state = state_with(&[(1, "dune", 0)]);
        assert!(
            state
                .add(request(2, Some(1), "dune", Lang::En), 0, 10)
                .is_ok()
        );
        assert_eq!(state.len(), 2);
    }

    #[test]
    fn a_user_is_capped_but_others_are_unaffected() {
        let mut state = State::default();
        state.add(request(1, Some(1), "a", Lang::En), 0, 2).unwrap();
        state.add(request(1, Some(1), "b", Lang::En), 0, 2).unwrap();

        assert_eq!(
            state.add(request(1, Some(1), "c", Lang::En), 0, 2),
            Err(RejectedWatch::TooMany)
        );
        assert!(state.add(request(2, Some(1), "c", Lang::En), 0, 2).is_ok());
    }

    #[test]
    fn removing_is_limited_to_the_owner() {
        let mut state = state_with(&[(1, "dune", 0)]);
        let id = state.for_user(1)[0].id;

        assert!(
            state.remove(id, 999).is_none(),
            "another user must not remove it"
        );
        assert_eq!(state.remove(id, 1).unwrap().query, "dune");
        assert!(state.is_empty());
    }

    #[test]
    fn removing_an_unknown_id_changes_nothing() {
        let mut state = state_with(&[(1, "dune", 0)]);
        assert!(state.remove(4_242, 1).is_none());
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn for_user_only_returns_that_users_watches() {
        let state = state_with(&[(1, "dune", 0), (2, "neuromancer", 0), (1, "hyperion", 0)]);
        let queries: Vec<&str> = state.for_user(1).iter().map(|w| w.query.as_str()).collect();
        assert_eq!(queries, ["dune", "hyperion"]);
    }

    #[test]
    fn marking_a_check_pushes_the_next_one_back_and_counts_it() {
        let mut state = state_with(&[(1, "dune", 0)]);
        let id = state.for_user(1)[0].id;

        state.mark_checked(id, HOUR);
        assert!(state.due(HOUR, HOUR).is_empty());
        assert_eq!(state.for_user(1)[0].checks, 1);
        assert_eq!(state.due(2 * HOUR, HOUR).len(), 1);
    }

    #[test]
    fn the_next_check_is_a_full_interval_after_creation() {
        let state = state_with(&[(1, "dune", 1_000)]);
        assert_eq!(state.seconds_until_next_due(1_000, HOUR), Some(HOUR));
        assert_eq!(
            state.seconds_until_next_due(1_000 + HOUR / 2, HOUR),
            Some(HOUR / 2)
        );
    }

    #[test]
    fn the_next_check_is_the_soonest_of_all_watches() {
        let mut state = State::default();
        state
            .add(request(1, Some(1), "late", Lang::En), 1_000, 10)
            .unwrap();
        state
            .add(request(1, Some(1), "soon", Lang::En), 500, 10)
            .unwrap();

        assert_eq!(state.seconds_until_next_due(1_000, HOUR), Some(HOUR - 500));
    }

    #[test]
    fn an_overdue_watch_reports_zero_rather_than_a_negative() {
        let state = state_with(&[(1, "dune", 0)]);
        assert_eq!(state.seconds_until_next_due(10 * HOUR, HOUR), Some(0));
    }

    #[test]
    fn a_watch_waiting_on_its_owner_is_not_counted() {
        let mut state = state_with(&[(1, "dune", 0)]);
        state.mark_notified(state.for_user(1)[0].id, 0);
        assert_eq!(state.seconds_until_next_due(0, HOUR), None);
    }

    #[test]
    fn an_empty_watchlist_has_no_next_check() {
        assert_eq!(State::default().seconds_until_next_due(0, HOUR), None);
    }

    #[test]
    fn marking_an_unknown_id_is_harmless() {
        let mut state = state_with(&[(1, "dune", 0)]);
        state.mark_checked(4_242, HOUR);
        assert_eq!(state.for_user(1)[0].checks, 0);
    }

    #[test]
    fn expiry_is_counted_from_creation_not_from_the_last_check() {
        let mut state = state_with(&[(1, "dune", 0)]);
        let id = state.for_user(1)[0].id;
        state.mark_checked(id, 20 * SECONDS_PER_DAY);

        let expired = state.prune_expired(30 * SECONDS_PER_DAY, 30 * SECONDS_PER_DAY);
        assert_eq!(expired.len(), 1);
        assert!(state.is_empty());
    }

    #[test]
    fn pruning_keeps_what_is_still_young() {
        let mut state = state_with(&[(1, "old", 0), (1, "new", 29 * SECONDS_PER_DAY)]);
        let expired = state.prune_expired(30 * SECONDS_PER_DAY, 30 * SECONDS_PER_DAY);

        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].query, "old");
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn a_notified_watch_stops_being_searched() {
        let mut state = state_with(&[(1, "dune", 0)]);
        let id = state.for_user(1)[0].id;
        assert_eq!(state.due(HOUR, HOUR).len(), 1);

        state.mark_notified(id, HOUR);
        assert!(state.due(10 * HOUR, HOUR).is_empty());
    }

    #[test]
    fn a_notified_watch_is_still_listed_until_it_is_downloaded() {
        let mut state = state_with(&[(1, "dune", 0)]);
        state.mark_notified(state.for_user(1)[0].id, HOUR);

        assert_eq!(state.for_user(1).len(), 1);
        assert!(state.for_user(1)[0].notified_at.is_some());
    }

    #[test]
    fn marking_an_unknown_id_as_notified_is_harmless() {
        let mut state = state_with(&[(1, "dune", 0)]);
        state.mark_notified(4_242, HOUR);
        assert!(state.for_user(1)[0].notified_at.is_none());
    }

    #[test]
    fn downloading_clears_the_matching_watch() {
        let mut state = state_with(&[(1, "dune", 0), (1, "hyperion", 0)]);
        let fulfilled = state.fulfil(1, "dune");

        assert_eq!(fulfilled.len(), 1);
        assert_eq!(fulfilled[0].query, "dune");
        assert_eq!(state.for_user(1).len(), 1);
    }

    #[test]
    fn matching_a_query_ignores_case_and_padding() {
        let mut state = state_with(&[(1, "dune", 0)]);
        assert_eq!(state.fulfil(1, "  DUNE  ").len(), 1);
    }

    #[test]
    fn downloading_does_not_touch_another_users_watch() {
        let mut state = state_with(&[(1, "dune", 0), (2, "dune", 0)]);
        assert_eq!(state.fulfil(1, "dune").len(), 1);
        assert_eq!(state.for_user(2).len(), 1);
    }

    #[test]
    fn downloading_something_unwatched_changes_nothing() {
        let mut state = state_with(&[(1, "dune", 0)]);
        assert!(state.fulfil(1, "neuromancer").is_empty());
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn a_guild_view_shows_every_owner_but_only_that_guild() {
        let mut state = State::default();
        state
            .add(request(1, Some(10), "dune", Lang::En), 0, 10)
            .unwrap();
        state
            .add(request(2, Some(10), "hyperion", Lang::En), 0, 10)
            .unwrap();
        state
            .add(request(3, Some(99), "elsewhere", Lang::En), 0, 10)
            .unwrap();

        let queries: Vec<&str> = state
            .for_guild(10)
            .iter()
            .map(|w| w.query.as_str())
            .collect();
        assert_eq!(queries, ["dune", "hyperion"]);
    }

    #[test]
    fn a_guild_view_ignores_watches_with_no_guild() {
        let mut state = State::default();
        state
            .add(request(1, None, "direct message", Lang::En), 0, 10)
            .unwrap();
        assert!(state.for_guild(10).is_empty());
    }

    #[test]
    fn a_moderator_can_stop_someone_elses_watch() {
        let mut state = State::default();
        state
            .add(request(1, Some(10), "dune", Lang::En), 0, 10)
            .unwrap();
        let id = state.for_guild(10)[0].id;

        assert_eq!(state.remove_within_guild(id, 10).unwrap().query, "dune");
        assert!(state.is_empty());
    }

    #[test]
    fn a_moderator_cannot_reach_into_another_guild() {
        let mut state = State::default();
        state
            .add(request(1, Some(99), "dune", Lang::En), 0, 10)
            .unwrap();
        let id = state.for_guild(99)[0].id;

        assert!(state.remove_within_guild(id, 10).is_none());
        assert_eq!(state.len(), 1);
    }

    #[tokio::test]
    async fn a_missing_file_loads_as_an_empty_watchlist() {
        let dir = tempfile::tempdir().unwrap();
        let list = Watchlist::load(dir.path().join("absent.json"))
            .await
            .unwrap();
        assert!(list.with(|s| s.is_empty()).await);
    }

    #[tokio::test]
    async fn watches_survive_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watchlist.json");

        let list = Watchlist::load(&path).await.unwrap();
        list.update(|s| {
            s.add(request(7, Some(1), "dune", Lang::En), 1_000, 10)
                .map(|w| w.id)
        })
        .await
        .unwrap()
        .unwrap();

        let reloaded = Watchlist::load(&path).await.unwrap();
        let watches = reloaded
            .with(|s| s.for_user(7).into_iter().cloned().collect::<Vec<_>>())
            .await;
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0].query, "dune");
        assert_eq!(watches[0].created_at, 1_000);
    }

    #[tokio::test]
    async fn ids_keep_increasing_across_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watchlist.json");

        let list = Watchlist::load(&path).await.unwrap();
        list.update(|s| {
            s.add(request(1, Some(1), "a", Lang::En), 0, 10)
                .map(|w| w.id)
        })
        .await
        .unwrap()
        .unwrap();
        list.update(|s| s.remove(1, 1)).await.unwrap();

        let reloaded = Watchlist::load(&path).await.unwrap();
        let id = reloaded
            .update(|s| {
                s.add(request(1, Some(1), "b", Lang::En), 0, 10)
                    .map(|w| w.id)
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            id, 2,
            "a reused id would confuse a user who still sees the old one"
        );
    }

    #[tokio::test]
    async fn missing_parent_directories_are_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deeper/watchlist.json");

        let list = Watchlist::load(&path).await.unwrap();
        list.update(|s| {
            s.add(request(1, Some(1), "dune", Lang::En), 0, 10)
                .map(|w| w.id)
        })
        .await
        .unwrap()
        .unwrap();
        assert!(path.exists());
    }

    #[tokio::test]
    async fn a_corrupt_file_is_reported_instead_of_being_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watchlist.json");
        tokio::fs::write(&path, b"{ this is not json")
            .await
            .unwrap();

        let error = Watchlist::load(&path).await.unwrap_err().to_string();
        assert!(error.contains("not a readable watchlist"), "got: {error}");
    }

    #[tokio::test]
    async fn the_language_is_stored_with_the_watch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watchlist.json");

        let list = Watchlist::load(&path).await.unwrap();
        list.update(|s| {
            s.add(request(1, Some(1), "dune", Lang::Fr), 0, 10)
                .map(|w| w.id)
        })
        .await
        .unwrap()
        .unwrap();

        let reloaded = Watchlist::load(&path).await.unwrap();
        assert_eq!(reloaded.with(|s| s.for_user(1)[0].lang).await, Lang::Fr);
    }

    #[tokio::test]
    async fn an_unreadable_path_is_an_error_not_an_empty_list() {
        // A directory where the file should be: losing the watches silently
        // would be worse than refusing to start.
        let dir = tempfile::tempdir().unwrap();
        assert!(Watchlist::load(dir.path()).await.is_err());
    }

    #[test]
    fn now_secs_is_a_plausible_unix_timestamp() {
        assert!(now_secs() > 1_700_000_000);
    }
}
