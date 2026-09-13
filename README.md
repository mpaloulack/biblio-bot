# biblio-bot

A Discord bot that searches ebooks through [Prowlarr](https://prowlarr.com) and
hands the one you pick to qBittorrent — in the right category, in the right
folder.

Type `/search dune`, get a list sorted by seeders, choose one from a menu, and
it starts downloading. It answers in English or French depending on each user's
Discord language.

> **Vibe coded.** This project was written by prompting an AI agent rather than
> by typing it out, and it shows in the shape of the repository: heavy test
> coverage and a lot of structure for a small bot. It runs on my own server, the
> tests are real and the CI gate is real — but read it with that in mind before
> trusting it with anything you care about. Issues and corrections are welcome.

## Status

`0.2.0` — beta. It runs, but the configuration surface may still move before
1.0. See the [changelog](CHANGELOG.md).

## Commands

| Command | What it does |
| --- | --- |
| `/search <query>` | Searches the configured categories, lists results by seeders, and offers a menu. Picking an entry sends it straight to qBittorrent. |
| `/stuck` | Lists your downloads in progress. Flags one as stuck, or resolves one already flagged — remove it, or search again for another release. |
| `/watchlist` | Lists the standing searches still running for you, and stops one. |
| `/watchlist-all` | Moderators only: lists every standing search on the server, whoever started it, and stops any of them. |
| `/status` | Checks that Prowlarr and qBittorrent answer, and shows which category and folder downloads will land in. |

French users get `/livre`, `/bloque`, `/veilles`, `/veilles-serveur` and `/etat`,
with French replies. The language follows
each user's own Discord locale, so the same bot can serve both in one server.
`DEFAULT_LOCALE` decides what everyone else gets.

## Standing searches

A book that is not out yet returns nothing, and retyping the same search every
week is exactly the kind of thing a bot should do instead.

When `/search` finds nothing, it offers a button. Press it and the bot keeps
looking on its own — four times a day by default — and posts in the channel,
mentioning you, as soon as something turns up.

The entry stays on your list after that: it is removed when you actually
**download** the book, not when it is found, so a notification you missed does
not quietly disappear. A search that never finds anything is dropped after
thirty days, and the bot says so rather than going silent.

`/watchlist` shows what is still running, how many times each has been checked
and how long it has left, and lets you stop one.

Anyone holding **Manage Server** also gets `/watchlist-all`, which lists every
standing search on that server with its owner and can stop any of them. It is
scoped to one server: a moderator never sees, or reaches, what was set up
somewhere else. Discord hides the command from everyone else, and the bot
re-checks the permission when it runs rather than trusting that.

The list is a plain JSON file — readable and editable from the host — kept on
the `/data` volume so it survives a restart.

## Downloads that finish, and downloads that do not

Handing a release to qBittorrent used to be the end of it: whether the file
ever arrived was something you found out by opening qBittorrent yourself.

Every download the bot sends is now tracked until it is done. It posts in the
channel it came from, mentioning you, once the torrent has finished and
started seeding — checked every five minutes by default.

Some downloads never get there. A release with no live seeders sits at a few
percent forever, and the bot cannot tell that apart from one that is merely
slow, so it does not guess: **you** say so. `/stuck` lists what is in
progress and flags the one that is going nowhere.

From then on the bot reminds you about it once a day, in the same channel,
until it is resolved. Running `/stuck` again and picking a flagged entry
offers two ways out:

- **Search again** — deletes the stalled torrent and its partial files, then
  re-runs the original search so you can pick a different release. The query
  is kept with the download for exactly this.
- **Remove** — deletes it and stops tracking, without searching again.

A flagged download is still checked for completion like any other, so one that
turns out to be slow rather than dead resolves itself and tells you.

Tracked downloads live in their own JSON file next to the standing searches,
on the same `/data` volume.

### Finding a torrent again

qBittorrent's add endpoint never answers with the hash of what it just added,
which leaves nothing obvious to follow a download by. Computing the info-hash
before sending is possible, but it means bencode parsing for `.torrent`
uploads and separate handling for magnets.

The bot tags instead: every download is added with a unique tag, and one
`torrents/info` call per sweep returns every torrent with its tags, so one
request covers all of them however many are in flight. It also means a
download the bot is tracking is recognisable in the qBittorrent UI.

## Requirements

- A [Prowlarr](https://prowlarr.com) instance with at least one indexer that
  carries book categories, and its API key.
- qBittorrent with its Web UI reachable from wherever the bot runs.
- A Discord application — see below.

## Setup

### 1. Create the Discord application

1. Open the [Discord developer portal](https://discord.com/developers/applications)
   and create an application.
2. In **Bot**, reset and copy the token. No privileged intent is required.
3. In **OAuth2 → URL Generator**, tick the `bot` and `applications.commands`
   scopes, then open the generated URL to invite the bot to your server.

### 2. Prepare a qBittorrent category

Create the category the bot will file downloads under (`ebooks` by default) and
give it a save path. See [the save path trap](#the-save-path-trap) for why the
path matters.

### 3. Configure

```bash
curl -O https://raw.githubusercontent.com/mpaloulack/biblio-bot/main/.env.example
mv .env.example .env
```

| Variable | Required | Default | Meaning |
| --- | --- | --- | --- |
| `DISCORD_TOKEN` | yes | — | Bot token from the developer portal. |
| `DISCORD_GUILD_ID` | no | — | Register commands in this server only, which is instant. Leave empty to register globally, which Discord takes up to an hour to propagate. |
| `PROWLARR_URL` | yes | — | e.g. `http://192.168.1.10:9696`. |
| `PROWLARR_API_KEY` | yes | — | Settings → General → API Key. |
| `QBIT_URL` | yes | — | e.g. `http://192.168.1.10:8081`. |
| `QBIT_USER` / `QBIT_PASS` | no | — | Only needed if you have not whitelisted the bot's subnet in qBittorrent. |
| `QBIT_CATEGORY` | no | `ebooks` | Category downloads are filed under. |
| `SEARCH_CATEGORIES` | no | `7020` | Newznab categories, comma separated. `7020` Books/EBook, `7000` Books, `7030` Comics, `7040` Technical, `3030` Audiobook. |
| `MAX_RESULTS` | no | `25` | Results offered, 1–25 (Discord's select menu limit). |
| `DEFAULT_LOCALE` | no | `en` | Language for users whose Discord locale is neither English nor French. |
| `WATCH_CHECKS_PER_DAY` | no | `4` | How often each standing search is retried, 1–24. |
| `WATCH_MAX_DAYS` | no | `30` | A standing search gives up after this many days, 1–365. |
| `WATCH_MAX_PER_USER` | no | `10` | Per-user limit on standing searches, 1–100. |
| `WATCHLIST_PATH` | no | `/data/watchlist.json` | Where the list is stored. The image already points this at its volume. |
| `DOWNLOAD_CHECK_INTERVAL_SECS` | no | `300` | How often tracked downloads are checked for having finished, 60–3600. |
| `DOWNLOADS_PATH` | no | `/data/downloads.json` | Where tracked downloads are stored, same convention as `WATCHLIST_PATH`. |
| `RUST_LOG` | no | `biblio_bot=info,serenity=warn` | Log filter. `biblio_bot=debug` shows each individual watch check. |

### 4. Run

```bash
docker compose up -d
```

Or without compose:

```bash
docker run -d --name biblio-bot --restart unless-stopped \
  --env-file .env -v biblio-data:/data ghcr.io/mpaloulack/biblio-bot:latest
```

The `/data` volume holds the standing searches and the downloads being tracked.
Without it both are lost on every restart — a download in flight stops being
watched, and nothing announces it when it lands. Everything else the bot does is
stateless.

Images are published for `linux/amd64` and `linux/arm64`. Tags: `latest` for the
newest release, `X.Y.Z` to pin one, `edge` for the tip of `main`.

The bot listens on nothing and only makes outbound connections, so it needs no
published port. It does need to reach Prowlarr and qBittorrent: if those run in
Docker on the same host, put all three on the same network and use service names
instead of IP addresses.

## Logs

On startup the bot prints what it is actually configured to talk to, which is
usually enough to spot a misconfiguration without reading the `.env`:

```
starting version=0.1.0
prowlarr=http://prowlarr:9696 qbit=http://gluetun:8081 qbit_auth=none ...
Prowlarr reachable version=2.5.2.5491
qBittorrent reachable version=v5.2.3
watchlist loaded watches=0 path=/data/watchlist.json
downloads loaded downloads=0 path=/data/downloads.json
```

Secrets are never printed: the token, the API key and the qBittorrent password
are reported only as present or absent. A test enforces that.

At `info` you get every command invocation with its user and channel, every
search with how many results it found, every download with where it landed, and
one line per sweep:

```
sweep finished total=3 due=1 hits=0 misses=1 failures=0 expired=0 next_check_in_secs=21598
```

`next_check_in_secs` answers the usual question — a standing search is only
retried a full interval after it was created, so with the default four checks a
day the first one happens six hours later, not immediately.

Set `RUST_LOG=biblio_bot=debug` to also see each individual check.

## The save path trap

qBittorrent only applies a category's **Save Path** when *Automatic Torrent
Management* is enabled, and it is off by default. A torrent added with just a
category — which is what most integrations do — therefore lands in qBittorrent's
default folder, not in the category's, silently.

This bot reads the save path of `QBIT_CATEGORY` and passes it explicitly with
every download, so the category setting is actually honoured. `/status` tells
you if the category is missing or has no path configured.

## Development

Requires a Rust toolchain; the version is pinned in `rust-toolchain.toml`.

```bash
cargo run                                  # run the bot
cargo test                                 # unit tests
cargo clippy --all-targets -- -D warnings  # lint
cargo fmt                                  # format
./scripts/coverage.sh                      # coverage, fails under 95%
```

### Layout

```
src/
  main.rs              bootstrap only
  lib.rs               module wiring and shared state
  config.rs            environment parsing and validation
  i18n.rs              every user-facing string, English and French
  prowlarr.rs          search and .torrent retrieval
  qbittorrent.rs       session, categories, adding and deleting downloads
  watchlist.rs         standing searches: scheduling rules and storage
  downloads.rs         tracked downloads: stuck flagging and storage
  ui.rs                embeds and menus — pure functions
  commands/            Discord interaction lifecycle
  watcher.rs           background sweep over due searches
  download_watcher.rs  background sweep over downloads in flight
```

The split is deliberate: `commands/` and the two watchers hold the gateway glue,
which cannot run without a live Discord connection, and delegate every decision
they make to `ui.rs`, `watchlist.rs` and `downloads.rs`. Everything else is unit
tested against a mock HTTP server, including both qBittorrent API generations,
the redirect handling for magnet links, the scheduling rules for standing
searches, and the daily gate on stuck reminders.

Coverage is measured over that testable surface — `src/main.rs`, `src/commands/`,
`src/watcher.rs` and `src/download_watcher.rs` are excluded — and CI fails below
95%. It currently sits at 99.7% of lines.

## Contributing

`main` is protected: it takes a pull request, and CI has to be green before the
merge button unlocks. Branches must be up to date with `main` and history stays
linear (squash merge only).

```bash
git switch -c my-change
# ...
gh pr create --fill
gh pr merge --squash --auto   # merges by itself once CI passes
```

Code, comments, commit messages, issues and documentation are in English. Only
the strings Discord users read are translated, and they all live in `i18n.rs` —
adding a language means adding a variant there and nowhere else.

## Licence

MIT — see [LICENSE](LICENSE).
