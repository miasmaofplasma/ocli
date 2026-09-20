# ocli

A terminal-native CLI for managing work-ticket notes in an Obsidian vault, written in Rust as a learning project that I would actually use.

## My current Obsidian workflow
- I have feature tickets living in <vault>/features/<feature>.md.  I use these notes extensivly for tracking progress, descisions, notes, and design while working on features.
- I have a Feature template note living in <vault>/templates/Feature.md
- I name my branches based on the features that are tracket in Jira.  The format I for my branch is always <FeatureType>-<TicketNumber>-<some-description>.
- When creating my feature notes in Obsidian, I use the QuickAdd plugin to fill out these details into my Feature template.
- When adding notes, I have to switch to my obsidian vault, add the note manually and continue.

## Why
Having to switch context to Obsidian in the middle of working in my terminal can break the flow of my coding process.  
A lot of times I just want to add a quick note or change the status in frontmatter and that requires me to go to Obsidain, hunt down the particular note that I'm working on, and make the update.
So instead, I decided to automate the process.  The current "context" of what note I'm working on is derived from the current branch that I'm on in git.  If I'm in the branch XXX-12345-fix-that-bug, then the note that I will be working on in Obsidian should be called XXX-12345.md (configurable).  
The `ocli` cli can create new notes (from template), edit frontmatter, add notes in particular sections (configurable) straight from the command line without me having to type in what note I'm working on every time I want to make an edit.

## How it works
- **The vault is the database.** `ocli` reads and writes the markdown files directly. No Obsidian app, no API, no sync service. Notes remain fully valid Obsidian documents.
- **Tickets are files.** A ticket is `features/<KEY>-<ID>.md` in the vault (e.g. `features/XXX-74043.md`). Listing tickets means listing the directory.
- **Git is the context.** The current branch must match a configurable regex whose named captures (e.g. `FeatureType`, `TicketNumber`) feed the template; the origin remote maps to the `repo` frontmatter field. Commands infer the ticket from where you are, with an explicit `--ticket` override.

### Write contract

- **Reads** anything: frontmatter, body, directory listings.
- **Writes** only two things:
  1. Frontmatter fields it manages (`status`, `done`, `Created`, `repo`, `owner`) — via surgical line edits; `fm` writes other fields per config types, never `ignore`-listed ones.
  2. CLI-owned `##` markdown sections it creates on demand (declared in config `[sections]`).
- Everything else in a note is never touched — in particular, ocli never writes past the `<!-- ocli:footer -->` footer marker; everything after it is plugin/widget territory (D8).

## Planned commands (v1)

| Command | Effect |
|---|---|
| `ocli new [--description "..."]` | Render the vault's QuickAdd template into `features/<ID>.md` — ticket inferred from the branch (explicit key still accepted); fills `repo`, `description`, and `status: "In Progress"`, `Created` via the template's `{{DATE}}`; refuses to overwrite |
| `ocli list [--status S] [--all-repos]` | Scan `features/`, print one block per ticket — id / description / status / repository, `-` for unset values; `--status` filters by a canonical status (a near-miss is a usage error); defaults to the current repo's tickets (D13), `--all-repos` lists everything; outside a repo the whole vault lists |
| `ocli status <Status>` | Update the inferred ticket's status; keeps `done:` in sync |
| `ocli fm <field> <value>` | General frontmatter setter — type rules from config, refuses managed and ignored fields |
| `ocli section <key>` | Print the current note's config-defined `##` section (`[sections]`) |
| `ocli section <key> add "<text>"` | Append an entry to that section (timestamped log, or checklist for `list` sections), creating the section at the footer marker when absent |
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

[sections]               # CLI-owned sections (heading + entry format: log | list)
questions = { heading = "## Open Questions", format = "list" }
todos     = { heading = "### Todo", format = "list" }

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
