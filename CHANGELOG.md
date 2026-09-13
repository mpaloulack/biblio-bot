# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] — 2026-09-13

Fixes for three faults that only showed up once the bot was actually running,
and the logging that would have made them obvious in the first place.

### Fixed

- **The watch button failed with *Unknown interaction*.** It saved the new
  standing search before answering Discord, and Discord allows three seconds
  to answer a component click. Both component handlers now acknowledge first
  and edit the reply once the work is done, which is what the download path
  already did.
- **`/watchlist` failed with *Cannot send an empty message*** when its menu
  timed out: the edit carried neither content nor an embed. It now keeps the
  listing on screen and says the selection expired.
- **A standing search survived its own download.** `State::fulfil` was written
  and unit tested but never actually called, so an entry stayed on the list
  until it expired thirty days later. It is now wired into the download path.

### Added

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
  each for watch creation, refusal and removal.
- `RUST_LOG` to control all of it, documented in the README and `.env.example`.

### Notes

All three fixes live in the Discord gateway glue, which is excluded from
coverage because it cannot run without a live connection. No test caught any
of them, and none would have: the gap is real and the logging above is the
mitigation.

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

[0.1.1]: https://github.com/mpaloulack/biblio-bot/releases/tag/v0.1.1
[0.1.0]: https://github.com/mpaloulack/biblio-bot/releases/tag/v0.1.0
