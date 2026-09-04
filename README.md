# ocli

A terminal-native CLI for managing work-ticket notes in an Obsidian vault, written in Rust as a learning project.

## Why

Workflow today: engineering work happens in the terminal; progress tracking, decisions, and product-bound questions live in Obsidian feature notes. Switching contexts to update a note breaks flow. `ocli` closes that gap: from inside a work repo on branch `BCP-74043-something`, the tool already knows which ticket you're on.

## How it works

- **The vault is the database.** `ocli` reads and writes the markdown files directly. No Obsidian app, no API, no sync service. Notes remain fully valid Obsidian documents (frontmatter, wikilinks, and plugin blocks render unchanged).
- **Tickets are files.** A ticket is `features/<KEY>-<ID>.md` in the vault (e.g. `features/BCP-74043.md`). Listing tickets means listing the directory.
- **Git is the context.** The current git branch is parsed for a ticket key (`BCP-74043`, `NGP-14234`, ...); the git remote/folder maps to the `repo` frontmatter field. Commands infer the ticket from where you are, with an explicit `--ticket` override.

### Write contract

The vault is shared with Obsidian and the Meta Bind plugin, so `ocli` is conservative:

- **Reads** anything: frontmatter, body, directory listings.
- **Writes** only two things:
  1. Frontmatter fields it owns (e.g. `status`, `done`, timestamps).
  2. CLI-owned `##` markdown sections it creates on demand (progress log, decisions, open questions).
- Everything else in a note is never touched.

## Planned commands (v1)

| Command | Effect |
|---|---|
| `ocli new <KEY-ID> "<title>"` | Scaffold a feature note: frontmatter + standard sections, inferring `repo` from the current repo |
| `ocli list` | Read `features/`, print ID / title / status; filter by status or current repo |
| `ocli status <status>` | Update the inferred ticket's status frontmatter |
| `ocli note "<text>"` | Append a timestamped entry to the inferred ticket's log section |
| `ocli questions` | Cross-ticket view of open questions worth taking to product |

Status vocabulary: `Backlog → In Progress → In Review → Complete`, plus occasional `Blocked`. Setting `status: Complete` also sets `done: true` (and vice versa), so Obsidian-side boolean filtering stays in sync.

## Development

- Rust (edition 2024), built with cargo; devenv/nix for tooling (`.envrc` + `devenv.nix`).
- Testing uses a fixture vault under `tests/` — never the real vault.
- Project status: **planning**. See `PLAN.md` for design decisions, remaining work, and open questions. `AGENTS.md` covers how AI assistance is used in this repo.
