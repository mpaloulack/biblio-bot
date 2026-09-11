# biblio-bot

A Discord bot that searches ebooks through [Prowlarr](https://prowlarr.com) and
hands the one you pick to qBittorrent — in the right category, in the right
folder.

Type `/search dune`, get a list sorted by seeders, choose one from a menu, and
it starts downloading. It answers in English or French depending on each user's
Discord language.

> **Vibe coded.** This project was written by prompting an AI agent rather than
> by typing it out, and it shows in the shape of the repository: heavy test
> coverage, long explanatory comments, a lot of structure for a small bot. It is
> used in production on my own server, the tests are real and the CI gate is
> real — but read it with that in mind before trusting it with anything you
> care about. Issues and corrections are welcome.

## Status

`0.0.1` — beta. It works, the API surface may still move.

## Commands

| Command | What it does |
| --- | --- |
| `/search <query>` | Searches the configured categories, lists results by seeders, and offers a menu. Picking an entry sends it straight to qBittorrent. |
| `/status` | Checks that Prowlarr and qBittorrent answer, and shows which category and folder downloads will land in. |

French users get `/livre` and `/etat`, with French replies. The language follows
each user's own Discord locale, so the same bot can serve both in one server.
`DEFAULT_LOCALE` decides what everyone else gets.

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

### 4. Run

```bash
docker compose up -d
```

Or without compose:

```bash
docker run -d --name biblio-bot --restart unless-stopped \
  --env-file .env ghcr.io/mpaloulack/biblio-bot:latest
```

Images are published for `linux/amd64` and `linux/arm64`. Tags: `latest` for the
newest release, `X.Y.Z` to pin one, `edge` for the tip of `main`.

The bot listens on nothing and only makes outbound connections, so it needs no
published port. It does need to reach Prowlarr and qBittorrent: if those run in
Docker on the same host, put all three on the same network and use service names
instead of IP addresses.

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
  main.rs          bootstrap only
  lib.rs           module wiring and shared state
  config.rs        environment parsing and validation
  i18n.rs          every user-facing string, English and French
  prowlarr.rs      search and .torrent retrieval
  qbittorrent.rs   session, categories, adding downloads
  ui.rs            embeds and menus — pure functions
  commands/        Discord interaction lifecycle
```

The split is deliberate: `commands/` holds the gateway glue, which cannot run
without a live Discord connection, and delegates every decision it makes to
`ui.rs`. Everything else is unit tested against a mock HTTP server, including
both qBittorrent API generations and the redirect handling for magnet links.

Coverage is measured over that testable surface — `src/main.rs` and
`src/commands/` are excluded — and CI fails below 95%. It currently sits at
100% of lines and functions.

## Contributing

Code, comments, commit messages, issues and documentation are in English. Only
the strings Discord users read are translated, and they all live in `i18n.rs` —
adding a language means adding a variant there and nowhere else.

## Licence

MIT — see [LICENSE](LICENSE).
