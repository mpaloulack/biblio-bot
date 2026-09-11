# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0]: https://github.com/mpaloulack/biblio-bot/releases/tag/v0.1.0
