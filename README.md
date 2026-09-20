# ocli

A terminal-native CLI for managing work-ticket notes in an Obsidian vault, written in Rust as a learning project.

## Why

Workflow today: engineering work happens in the terminal; progress tracking, decisions, and product-bound questions live in Obsidian feature notes. Switching contexts to update a note breaks flow. `ocli` closes that gap: from inside a work repo on branch `BCP-74043-something`, the tool already knows which ticket you're on.

## How it works

- **The vault is the database.** `ocli` reads and writes the markdown files directly. No Obsidian app, no API, no sync service. Notes remain fully valid Obsidian documents (frontmatter, wikilinks, and plugin blocks render unchanged).
- **Tickets are files.** A ticket is `features/<KEY>-<ID>.md` in the vault (e.g. `features/BCP-74043.md`). Listing tickets means listing the directory.
- **Git is the context.** The current branch must match a configurable regex whose named captures (e.g. `FeatureType`, `TicketNumber`) feed the template; the origin remote maps to the `repo` frontmatter field. Commands infer the ticket from where you are, with an explicit `--ticket` override.

### Write contract

The vault is shared with Obsidian and the Meta Bind plugin, so `ocli` is conservative:

- **Reads** anything: frontmatter, body, directory listings.
- **Writes** only two things:
  1. Frontmatter fields it manages (`status`, `done`, `Created`, `repo`, `owner`) — via surgical line edits; `fm` writes other fields per config types, never `ignore`-listed ones.
  2. CLI-owned `##` markdown sections it creates on demand (Progress, Notes, Decisions, Open Questions — names from config).
- Everything else in a note is never touched — in particular, ocli never writes past the `<!-- ocli:footer -->` footer marker; everything after it is plugin/widget territory (D8).

## Planned commands (v1)

| Command | Effect |
|---|---|
| `ocli new [--description "..."]` | Render the vault's QuickAdd template into `features/<ID>.md` — ticket inferred from the branch (explicit key still accepted); fills `repo`, `description`, and `status: "In Progress"`, `Created` via the template's `{{DATE}}`; refuses to overwrite |
| `ocli list [--status S] [--all-repos]` | Scan `features/`, print one block per ticket — id / description / status / repository, `-` for unset values; `--status` filters by a canonical status (a near-miss is a usage error); defaults to the current repo's tickets (D13), `--all-repos` lists everything; outside a repo the whole vault lists |
| `ocli status <Status>` | Update the inferred ticket's status; keeps `done:` in sync |
| `ocli fm <field> <value>` | General frontmatter setter — type rules from config, refuses managed and ignored fields |
| `ocli progress / note / decision / question "<text>"` | Append timestamped entries to the ticket's configured `##` sections |
| `ocli questions [--all]` | Cross-vault view of unchecked Open Questions entries |
| `ocli open` | Open the current ticket's note in Obsidian (via the `obsidian://` URI scheme; requires the note to exist) |

Status vocabulary: `Backlog → In Progress → In Review → Complete`, plus occasional `Blocked` — validated by `ocli status`, which keeps `done:` in sync with `Complete`. A note whose status isn't in the vocabulary still lists (shown verbatim); `--status`/`status` reject it (D37).

## Configuration

All team-convention behavior lives in the config file (default `~/.config/ocli/config.toml`, overridable via `OCLI_CONFIG`) so convention drift is a config edit, not a code change. The vault root resolves with strict precedence: `--vault` flag > `OCLI_VAULT` > `[vault] root` — the config file is optional when a vault root comes from elsewhere.

```toml

[vault]
root = "/path/to/vault"
features_dir = "notes/features"
people_dir   = "notes/people"

[template]
path = "templates/Feature.md"

[template.values]        # static {{VALUE:...}} defaults
SprintNumber = "2026.1"

[frontmatter]            # field types for `ocli fm`: string | int | float | bool | olink | list<T>
estimate = "int"
ignore = ["relates-to", "blocked-by"]

[sections]               # CLI-owned ## sections
progress  = "Progress"
notes     = "Notes"
decisions = "Decisions"
questions = "Open Questions"

[tickets]
branch_pattern = '^(?<FeatureType>[A-Z]+)-(?<TicketNumber>\d+)'
id             = '{FeatureType}-{TicketNumber}'
```

`ocli new` assembles template values from four layers, most specific wins: `--set` flags → the explicit key, when passed (`FeatureType=BCP`, `TicketNumber=13423`) → branch named captures → `[template.values]`. The ticket ID comes from the key when given, otherwise from `[tickets] id` composed over the branch captures.

## Development

- Rust (edition 2024), built with cargo; devenv/nix for tooling (`.envrc` + `devenv.nix`).
- Testing uses a fixture vault under `tests/` — never the real vault.
- Key dependencies: `clap`, `yaml_serde`, `toml`, `gix` (gitoxide — pure-Rust git access), `directories`, `regex`, `chrono`, `thiserror`/`color-eyre`, `tracing` + `tracing-subscriber`.
- Project status: **Phase 5 complete** — config (parse/validate/three-source resolution), the markdown span locator, the frontmatter read projection (a typed `Status` with a read-faithful `Unknown` fallback — D37), `Note` loading, the features-directory scan, the git layer, the create path (`new`: branch→id inference, template rendering with the four-layer value map, `repo`/`description`/`status` fills, refuse-to-overwrite write), `list` with D13 repo filtering, and `open` (the `obsidian://` URI opener) are landed and tested — 139 tests across unit and integration tiers; clippy clean. Git specifics: a `gix` adapter (branch/origin discovery) behind a crate-private wall with `context()` as its only fact, and the `Context` object carries the git snapshot (branch + repo identity) derived tolerantly from the working directory — every command can ask "which ticket am I on, which repo is this" without touching git internals. Next: Phase 6, the write path (`status`, `fm`, the `progress`/`note`/`decision`/`question` appends). See `PLAN.md` for design decisions, remaining work, and open questions.
