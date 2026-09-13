//! Torrents sent to qBittorrent, tracked until they finish seeding.
//!
//! qBittorrent's add endpoint never returns a hash, so each download is
//! tagged with its own id instead ([`Download::tag`]); that tag is the only
//! way the background sweep can find it again later.

use crate::i18n::Lang;
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Download {
    pub id: u64,
    pub user_id: u64,
    pub channel_id: u64,
    #[serde(default)]
    pub guild_id: Option<u64>,
    /// Kept so a stuck entry can be searched again.
    pub query: String,
    /// The release actually picked, for display.
    pub title: String,
    /// "biblio-<id>", sent to qBittorrent as the torrent's tag.
    pub tag: String,
    pub added_at: i64,
    pub last_checked_at: i64,
    #[serde(default)]
    pub lang: Lang,
    /// Set by `/stuck`. Purely informational: the sweep keeps checking for
    /// completion regardless, in case it finishes anyway.
    #[serde(default)]
    pub stuck_marked_at: Option<i64>,
    /// Last time this entry was included in the daily stuck digest.
    #[serde(default)]
    pub last_digest_at: Option<i64>,
}

/// What a caller supplies to start tracking a download; the rest is bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDownload {
    pub user_id: u64,
    pub channel_id: u64,
    pub guild_id: Option<u64>,
    pub query: String,
    pub title: String,
    pub lang: Lang,
}

/// The whole file, and every decision taken on it. Synchronous and pure, so
/// the rules can be tested without a runtime or a filesystem.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    next_id: u64,
    downloads: Vec<Download>,
}

impl State {
    pub fn add(&mut self, new: NewDownload, now: i64) -> &Download {
        self.next_id += 1;
        let id = self.next_id;
        self.downloads.push(Download {
            id,
            user_id: new.user_id,
            channel_id: new.channel_id,
            guild_id: new.guild_id,
            query: new.query,
            title: new.title,
            tag: format!("biblio-{id}"),
            lang: new.lang,
            added_at: now,
            last_checked_at: now,
            stuck_marked_at: None,
            last_digest_at: None,
        });
        self.downloads.last().expect("just pushed")
    }

    pub fn for_user(&self, user_id: u64) -> Vec<&Download> {
        self.downloads
            .iter()
            .filter(|d| d.user_id == user_id)
            .collect()
    }

    /// Every tracked download, for the sweep that checks all of them each tick.
    pub fn all(&self) -> &[Download] {
        &self.downloads
    }

    /// Removes a download, but only for the user tracking it.
    pub fn remove(&mut self, id: u64, user_id: u64) -> Option<Download> {
        let index = self
            .downloads
            .iter()
            .position(|d| d.id == id && d.user_id == user_id)?;
        Some(self.downloads.remove(index))
    }

    /// Removes without checking ownership: the background sweep confirming a
    /// download is finished, or rolling back a reservation `qbit.add` failed
    /// to honour.
    pub fn take(&mut self, id: u64) -> Option<Download> {
        let index = self.downloads.iter().position(|d| d.id == id)?;
        Some(self.downloads.remove(index))
    }

    /// Only the owner can flag their own download. Returns whether it found one.
    pub fn mark_stuck(&mut self, id: u64, user_id: u64, now: i64) -> bool {
        let Some(download) = self
            .downloads
            .iter_mut()
            .find(|d| d.id == id && d.user_id == user_id)
        else {
            return false;
        };
        download.stuck_marked_at = Some(now);
        true
    }

    pub fn mark_checked(&mut self, id: u64, now: i64) {
        if let Some(download) = self.downloads.iter_mut().find(|d| d.id == id) {
            download.last_checked_at = now;
        }
    }

    /// Stuck downloads not yet mentioned today.
    pub fn due_for_digest(&self, now: i64, interval: i64) -> Vec<Download> {
        self.downloads
            .iter()
            .filter(|d| {
                d.stuck_marked_at.is_some()
                    && d.last_digest_at.is_none_or(|last| now - last >= interval)
            })
            .cloned()
            .collect()
    }

    pub fn mark_digested(&mut self, id: u64, now: i64) {
        if let Some(download) = self.downloads.iter_mut().find(|d| d.id == id) {
            download.last_digest_at = Some(now);
        }
    }

    pub fn len(&self) -> usize {
        self.downloads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.downloads.is_empty()
    }
}

/// The file-backed download list shared by the commands and the background sweep.
#[derive(Debug)]
pub struct Downloads {
    path: PathBuf,
    state: Mutex<State>,
}

impl Downloads {
    /// A missing file is an empty list; an unreadable one is an error, because
    /// silently starting empty would throw away in-progress tracking.
    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let state = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("{} is not a readable downloads file", path.display()))?,
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
    use crate::watchlist::now_secs;

    const DAY: i64 = 86_400;

    fn request(user_id: u64, query: &str, title: &str) -> NewDownload {
        NewDownload {
            user_id,
            channel_id: 100,
            guild_id: Some(1),
            query: query.to_owned(),
            title: title.to_owned(),
            lang: Lang::En,
        }
    }

    fn state_with(entries: &[(u64, &str, &str)]) -> State {
        let mut state = State::default();
        for (user, query, title) in entries {
            state.add(request(*user, query, title), 0);
        }
        state
    }

    #[test]
    fn adding_assigns_increasing_ids_and_derived_tags() {
        let state = state_with(&[(1, "dune", "Dune"), (1, "hyperion", "Hyperion")]);
        let downloads = state.for_user(1);
        assert_eq!(downloads[0].id, 1);
        assert_eq!(downloads[0].tag, "biblio-1");
        assert_eq!(downloads[1].id, 2);
        assert_eq!(downloads[1].tag, "biblio-2");
    }

    #[test]
    fn all_returns_every_tracked_download() {
        let state = state_with(&[(1, "dune", "Dune"), (2, "hyperion", "Hyperion")]);
        assert_eq!(state.all().len(), 2);
    }

    #[test]
    fn for_user_only_returns_that_users_downloads() {
        let state = state_with(&[(1, "dune", "Dune"), (2, "hyperion", "Hyperion")]);
        assert_eq!(state.for_user(1).len(), 1);
        assert_eq!(state.for_user(2).len(), 1);
    }

    #[test]
    fn removing_is_limited_to_the_owner() {
        let mut state = state_with(&[(1, "dune", "Dune")]);
        let id = state.for_user(1)[0].id;

        assert!(
            state.remove(id, 999).is_none(),
            "another user must not remove it"
        );
        assert_eq!(state.remove(id, 1).unwrap().title, "Dune");
        assert!(state.is_empty());
    }

    #[test]
    fn take_removes_regardless_of_owner() {
        let mut state = state_with(&[(1, "dune", "Dune")]);
        let id = state.for_user(1)[0].id;

        assert_eq!(state.take(id).unwrap().title, "Dune");
        assert!(state.is_empty());
    }

    #[test]
    fn take_of_an_unknown_id_changes_nothing() {
        let mut state = state_with(&[(1, "dune", "Dune")]);
        assert!(state.take(4_242).is_none());
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn mark_stuck_is_limited_to_the_owner() {
        let mut state = state_with(&[(1, "dune", "Dune")]);
        let id = state.for_user(1)[0].id;

        assert!(
            !state.mark_stuck(id, 999, 10),
            "another user must not flag it"
        );
        assert!(state.for_user(1)[0].stuck_marked_at.is_none());

        assert!(state.mark_stuck(id, 1, 10));
        assert_eq!(state.for_user(1)[0].stuck_marked_at, Some(10));
    }

    #[test]
    fn mark_stuck_of_an_unknown_id_is_harmless() {
        let mut state = state_with(&[(1, "dune", "Dune")]);
        assert!(!state.mark_stuck(4_242, 1, 10));
    }

    #[test]
    fn only_stuck_downloads_are_due_for_a_digest() {
        let mut state = state_with(&[(1, "dune", "Dune"), (1, "hyperion", "Hyperion")]);
        let dune = state.for_user(1)[0].id;
        state.mark_stuck(dune, 1, 0);

        let due = state.due_for_digest(DAY, DAY);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].title, "Dune");
    }

    #[test]
    fn a_download_digested_today_is_not_due_again() {
        let mut state = state_with(&[(1, "dune", "Dune")]);
        let id = state.for_user(1)[0].id;
        state.mark_stuck(id, 1, 0);
        state.mark_digested(id, DAY);

        assert!(state.due_for_digest(DAY + 100, DAY).is_empty());
    }

    #[test]
    fn a_download_digested_a_full_day_ago_is_due_again() {
        let mut state = state_with(&[(1, "dune", "Dune")]);
        let id = state.for_user(1)[0].id;
        state.mark_stuck(id, 1, 0);
        state.mark_digested(id, 0);

        assert_eq!(state.due_for_digest(DAY, DAY).len(), 1);
    }

    #[test]
    fn mark_digested_of_an_unknown_id_is_harmless() {
        let mut state = state_with(&[(1, "dune", "Dune")]);
        state.mark_digested(4_242, 10);
        assert!(state.for_user(1)[0].last_digest_at.is_none());
    }

    #[test]
    fn mark_checked_updates_the_timestamp() {
        let mut state = state_with(&[(1, "dune", "Dune")]);
        let id = state.for_user(1)[0].id;
        state.mark_checked(id, 500);
        assert_eq!(state.for_user(1)[0].last_checked_at, 500);
    }

    #[tokio::test]
    async fn a_missing_file_loads_as_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let list = Downloads::load(dir.path().join("absent.json"))
            .await
            .unwrap();
        assert!(list.with(|s| s.is_empty()).await);
    }

    #[tokio::test]
    async fn downloads_survive_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("downloads.json");

        let list = Downloads::load(&path).await.unwrap();
        list.update(|s| s.add(request(7, "dune", "Dune"), 1_000).id)
            .await
            .unwrap();

        let reloaded = Downloads::load(&path).await.unwrap();
        let downloads = reloaded
            .with(|s| s.for_user(7).into_iter().cloned().collect::<Vec<_>>())
            .await;
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].title, "Dune");
        assert_eq!(downloads[0].added_at, 1_000);
    }

    #[tokio::test]
    async fn ids_keep_increasing_across_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("downloads.json");

        let list = Downloads::load(&path).await.unwrap();
        list.update(|s| s.add(request(1, "a", "A"), 0).id)
            .await
            .unwrap();
        list.update(|s| s.remove(1, 1)).await.unwrap();

        let reloaded = Downloads::load(&path).await.unwrap();
        let id = reloaded
            .update(|s| s.add(request(1, "b", "B"), 0).id)
            .await
            .unwrap();
        assert_eq!(
            id, 2,
            "a reused id would confuse a user who still sees the old one"
        );
    }

    #[tokio::test]
    async fn missing_parent_directories_are_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deeper/downloads.json");

        let list = Downloads::load(&path).await.unwrap();
        list.update(|s| s.add(request(1, "dune", "Dune"), 0).id)
            .await
            .unwrap();
        assert!(path.exists());
    }

    #[tokio::test]
    async fn a_corrupt_file_is_reported_instead_of_being_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("downloads.json");
        tokio::fs::write(&path, b"{ this is not json")
            .await
            .unwrap();

        let error = Downloads::load(&path).await.unwrap_err().to_string();
        assert!(
            error.contains("not a readable downloads file"),
            "got: {error}"
        );
    }

    #[tokio::test]
    async fn an_unreadable_path_is_an_error_not_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Downloads::load(dir.path()).await.is_err());
    }

    #[test]
    fn now_secs_is_reused_from_watchlist() {
        assert!(now_secs() > 1_700_000_000);
    }
}
