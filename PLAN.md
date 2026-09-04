# ocli — Plan

Living document: design decisions, remaining work, open questions. Update it as decisions land; the AI assistant maintains it alongside the code.

## Goals

1. Ship a CLI the owner uses daily for ticket tracking against their Obsidian vault.
2. Learn project-shaped Rust: module organization, error handling, serde, clap, integration testing — with standard crates, understanding what they do underneath.

## Confirmed requirements

- Vault markdown files are the sole source of truth; no Obsidian API or app interaction.
- A ticket is `features/<KEY>-<ID>.md`; the ID is the filename. Listing = directory read.
- Ticket inference from the current git branch (`[A-Z]+-\d+` pattern); explicit `--ticket` flag overrides; repo identity inferred from git remote/folder and matched against the `repo` frontmatter field.
- Read anything, write only: owned frontmatter fields + CLI-owned `##` sections created on demand. Never touch user prose, wikilinks, or Meta Bind blocks.
- Status vocabulary: `Backlog`, `In Progress`, `In Review`, `Complete`, `Blocked`. `status: Complete` ⇔ `done: true` kept in sync.
- v1 command set: `new`, `list`, `status`, `note`, `questions`.

## Design decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | Direct filesystem access, no Obsidian/HTTP layer | Vault is a plain folder of markdown; removes a whole dependency class; better learning surface (file IO, error handling) |
| D2 | CLI-owned `##` markdown sections; bold-paragraph pseudo-headings are read-only | `##` headings are trivially and reliably parseable anchors; existing notes keep their organic `**bold**` style, untouched |
| D3 | Standard crates: `clap` (derive), `serde` + a YAML crate for frontmatter, `regex`, `anyhow` (app errors) + `thiserror` (library-style errors) | Boring and idiomatic; each is a standard learning topic in itself |
| D4 | Ticket ID lives in the filename; frontmatter `aliases` carries it too | Matches the existing vault convention (BCP-74043.md aliases BCP-74043) |
| D5 | Frontmatter edits rewrite only owned fields; unknown fields preserved | Notes are shared with Meta Bind widgets (`relates-to`, `blocked-by`); clobbering them would be data loss |
| D6 | Integration tests run against a fixture vault copied to a temp dir | The real vault is user data; tests must be isolated and full-suite-safe |

## Remaining work

Phased; each phase ends with something runnable.

- [ ] **Phase 0 — hygiene:** commit current scaffold, `.gitignore` (`/target`, `/.direnv/`), pick YAML crate, add deps.
- [ ] **Phase 1 — config & vault path:** resolve vault root (flag > env var > config file — see Q1). `ocli list` fails with a clear error when unresolved.
- [ ] **Phase 2 — read path:** frontmatter model (`serde` structs, optional fields, wikilinks as plain strings), `list` command with status filter. Fixture vault in `tests/`.
- [ ] **Phase 3 — create path:** `new` scaffolds frontmatter + body sections, infers `repo` from git remote.
- [ ] **Phase 4 — write path:** `status` (frontmatter edit with round-trip fidelity, D5) and `note` (append timestamped entry to CLI-owned section).
- [ ] **Phase 5 — git context:** branch-name ticket inference + `--ticket` override; `status`/`note`/`questions` become ID-free.
- [ ] **Phase 6 — cross-ticket views:** `questions` aggregates open questions across `features/`.
- [ ] **Phase 7 — polish:** `clippy`/`fmt` clean, shell completions, help text.

## Open questions

- **Q1 — Vault path resolution:** flag `--vault`, env `OCLI_VAULT`, or config file (`~/.config/ocli/config.toml`)? Leaning: env + config file, flag as override. Leaning recorded; decision at Phase 1.
- **Q2 — CLI-owned section names:** e.g. `## Log`, `## Decisions`, `## Open Questions` — exact names and whether one log or separate sections. Decision at Phase 4.
- **Q3 — Frontmatter round-trip:** a YAML crate re-serializing frontmatter may reorder/reformat the block. Acceptable? Or preserve original formatting and do surgical line edits? Decision at Phase 4; affects crate choice.
- **Q4 — Note types:** current notes carry `type: BCP`; is v1 features-only, or must `list` handle other folders? Assumed features-only for v1.
- **Q5 — `repo` matching:** git remote URL → repo name → match against the wikilink target in `repo: "[[name]]"`? Confirm matching rule at Phase 5.
- **Q6 — `estimate`/`sprint` fields:** leave alone, or manage? Assumed leave alone in v1.

## Learning log

Rust topics this project exercises, to be ticked off as encountered:

- [ ] Ownership/borrowing review in real context (file IO lifetimes)
- [ ] Error handling: `anyhow` vs `thiserror`, `?`, custom error enums
- [ ] `serde` data modeling with optional fields and enums
- [ ] `clap` derive API and subcommands
- [ ] Module layout for a multi-command binary
- [ ] Regex, string parsing (branch names, frontmatter)
- [ ] Integration testing with temp fixtures (`tempfile` or std)
- [ ] Reading git state without shelling out (`.git/HEAD`, refs) — crate TBD at Phase 5
