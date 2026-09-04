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
- Everything else in a note is never touched.

## Planned commands (v1)

| Command | Effect |
|---|---|
| `ocli new <KEY-ID> [title]` | Render the vault's QuickAdd template into `features/<KEY-ID>.md` (requires being on a matching ticket branch): placeholder substitution from the value map, `repo` from the origin remote, `Created` from the branch's first commit, `owner` from commit authors; refuses to overwrite |
| `ocli list [--status S] [--all-repos]` | Scan `features/`, print ID / title / status / repo; defaults to the current repo's tickets |
| `ocli status <Status>` | Update the inferred ticket's status; keeps `done:` in sync |
| `ocli fm <field> <value>` | General frontmatter setter — type rules from config, refuses managed and ignored fields |
| `ocli progress / note / decision / question "<text>"` | Append timestamped entries to the ticket's configured `##` sections |
| `ocli questions [--all]` | Cross-vault view of unchecked Open Questions entries |

Status vocabulary: `Backlog → In Progress → In Review → Complete`, plus occasional `Blocked` — validated by `ocli status`, which keeps `done:` in sync with `Complete`.

## Configuration

All team-convention behavior lives in `~/.config/ocli/config.toml` (located via the `directories` crate; `--vault` overrides the vault path) so convention drift is a config edit, not a code change:

```toml
vault = "/path/to/vault"

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
```

`ocli new` assembles template values from four layers, most specific wins: `--set` flags → the ticket key (`FeatureType=BCP`, `TicketNumber=13423`) → branch named captures → `[template.values]`.

## Development

- Rust (edition 2024), built with cargo; devenv/nix for tooling (`.envrc` + `devenv.nix`).
- Testing uses a fixture vault under `tests/` — never the real vault.
- Key dependencies: `clap`, `yaml_serde`, `toml`, `gix` (gitoxide — pure-Rust git access), `directories`, `regex`, `anyhow`/`thiserror`.
- Project status: **planning**. See `PLAN.md` for design decisions, remaining work, and open questions. `AGENTS.md` covers how AI assistance is used in this repo.
