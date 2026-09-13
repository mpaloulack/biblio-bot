# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] — 2026-09-13

Downloads no longer disappear once they are handed over: the bot follows them
to completion and says so. Plus three fixes for faults that only showed up
against a real Discord, and the logging that would have made them obvious.

### Added

- **Downloads tracked to completion.** Every release sent to qBittorrent is
  followed until it finishes, and the bot posts in the channel it came from,
  mentioning you, once it starts seeding. Checked every five minutes by
  default (`DOWNLOAD_CHECK_INTERVAL_SECS`).
- **`/stuck`** (`/bloque` in French) — a download with no live seeders is
  indistinguishable from a merely slow one, so the bot does not guess: you
  flag it. A flagged download is mentioned once a day until it is resolved,
  and picking it again offers to delete it, or to delete it and re-run the
  original search for a different release. It is still checked for completion
  throughout, so one that was only slow resolves itself.
- **A startup summary.** The bot prints the configuration it will actually use
  — which Prowlarr, which qBittorrent, which category and folder — so a
  misconfiguration is visible without reading the `.env`. Secrets are reported
  as present or absent and never echoed; a test enforces that.
- **One line per sweep**, including `next_check_in_secs`. A standing search is
  only retried a full interval after it is created, so with the default four
  checks a day nothing happens for six hours — which otherwise reads exactly
  like a broken feature.
- **Command logging**: each invocation with its user and channel, how long it
  took, how many results a search found, where a download landed, and one line
  each for watch creation, refusal and removal. `RUST_LOG` controls all of it.

### Fixed

- **The watch button failed with *Unknown interaction*.** It saved the new
  standing search before answering Discord, and Discord allows three seconds
  to answer a component click. Both component handlers now acknowledge first
  and edit the reply once the work is done.
- **`/watchlist` failed with *Cannot send an empty message*** when its menu
  timed out: the edit carried neither content nor an embed. It now keeps the
  listing on screen and says the selection expired.
- **A standing search survived its own download.** `State::fulfil` was written
  and unit tested but never actually called, so an entry stayed on the list
  until it expired thirty days later.

### Notes

qBittorrent's `/torrents/add` never returns the hash of what it just added,
which leaves nothing obvious to follow a download by. Rather than parse
bencode to compute the info-hash — and handle magnets separately — each
download is added with its own tag, and a single `/torrents/info` call per
sweep covers every download in flight.

`DOWNLOADS_PATH` joins `WATCHLIST_PATH` on the `/data` volume. Without that
volume a download in flight stops being watched across a restart, and nothing
announces it when it lands.

All three fixes above live in the Discord gateway glue, which is excluded from
coverage because it cannot run without a live connection. None was caught by a
test, and none would have been: the logging is the mitigation.

## [0.1.0] — 2026-09-11

First published release. Beta: it runs, but the configuration surface may still
move before 1.0.

### Added

- `/search` — searches Prowlarr for ebooks, lists results by seeders and sends
  the one you pick to qBittorrent.
- `/status` — reports whether Prowlarr and qBittorrent answer, and which
  category and folder downloads land in.
- **Standing searches.** A search that finds nothing offers to keep looking.
  The bot retries on its own, posts in the channel as soon as something turns
  up, and drops the entry once the book is actually downloaded rather than when
  it is found. `/watchlist` manages your own; `/watchlist-all` lets anyone with
  *Manage Server* manage every search on that server.
- English and French, picked from each user's own Discord locale, with
  localised command names (`/livre`, `/veilles`, `/veilles-serveur`, `/etat`).
- Multi-architecture container images on GitHub Container Registry for
  `linux/amd64` and `linux/arm64`.

### Notes

Two behaviours of the target APIs are handled explicitly, because both fail
silently otherwise:

- qBittorrent ignores a category's save path unless *Automatic Torrent
  Management* is enabled, and it is off by default. The save path is therefore
  resolved and sent with every download.
- `/torrents/add` answers `Ok.` on qBittorrent 4.x and a JSON object on 5.x.

[0.2.0]: https://github.com/mpaloulack/biblio-bot/releases/tag/v0.2.0
[0.1.0]: https://github.com/mpaloulack/biblio-bot/releases/tag/v0.1.0
