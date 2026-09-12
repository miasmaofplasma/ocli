# ocli — Plan

Living document: design decisions, remaining work, open questions. Rules for this file:

- A decision gets a D-number when it's needed, not before. Record *what* and *why*, briefly. Details that only matter at implementation time get decided then.
- D-numbers are stable — code comments and the README cite them.
- Implementation status lives in the phase checklist, one line per landed phase. The *how* lives in the code, tests, and git history, not here. (Full-length decision essays from earlier phases are in the PLAN.md git history.)
- A contradiction between this file and the code is a bug in one of the two — fix both together.

## Goals

1. Ship a CLI the owner uses daily for ticket tracking against their Obsidian vault.
2. Learn project-shaped Rust: module organization, error handling, serde, clap, integration testing — standard crates, understood underneath, not cargo-culted.
3. Specific by default, general in mechanism: team conventions (branch patterns, frontmatter types, section names, ignore lists, template) live in config, so convention drift is a config edit, never a code change.

## Confirmed requirements

- Vault markdown files are the sole source of truth; no Obsidian API or app interaction.
- A ticket is `features/<KEY>-<ID>.md`; the ID is the filename. Listing = directory read.
- Ticket inference from the current git branch via a configurable pattern; `--ticket` overrides. Repo identity from the origin remote.
- Read anything; write only owned frontmatter fields and CLI-owned `##` sections. Never touch user prose, wikilinks, Meta Bind blocks, or anything past the footer marker (D8).
- Status vocabulary: `Backlog`, `In Progress`, `In Review`, `Complete`, `Blocked`; `Complete` ⇔ `done: true` kept in sync.
- v1 command set: `new`, `list`, `status`, `fm`, `progress`/`note`/`decision`/`question`, `questions`, `open` (README has the full table).

## Design decisions

### Foundations

- **D1 — direct filesystem access.** The vault is a plain folder of markdown; no Obsidian/HTTP layer. Removes a dependency class; file IO and error handling are the learning surface.
- **D2 — ocli owns `##` sections.** `##` headings are reliably parseable anchors; existing bold-paragraph pseudo-headings stay read-only.
- **D3 —** early crate sketch; superseded by D28.
- **D4 — ticket ID = filename.** `aliases` carries it too (existing vault convention).
- **D5 — write only owned fields.** Unknown frontmatter is preserved verbatim; Meta Bind widgets (`relates-to`, `blocked-by`) have another owner — clobbering them is data loss.
- **D6 — fixture vaults only.** Integration tests copy `tests/fixtures/` into temp dirs; the real vault is never touched by any command, test, or example.
- **D9 — `yaml_serde`** for frontmatter. `serde_yaml` is archived; `serde_yml` is unsound (RUSTSEC-2025-0068); this is the maintained fork.
- **D10 — sync IO, no async runtime.** File ops are microseconds; nothing waits. Revisit only if a network feature lands.
- **D28 — dependency set** (supersedes D3): `clap` (derive), `serde`, `toml`, `directories`, `regex`, `thiserror`, `color-eyre`, `tracing` + `tracing-subscriber` (env-filter), `yaml_serde 0.10`, `gix 0.87` (pinned, D14); dev `tempfile`. Deliberately not adopted: `tokio`, `async-trait`, `sqlx` — nothing here waits (D1/D10).

### Config

- **D7 — config location:** `~/.config/ocli/config.toml` via `directories` (`ProjectDirs`); `OCLI_CONFIG` overrides.
- **D30 — vault root precedence:** `--vault` > `OCLI_VAULT` > `[vault] root`. Grammar/policy error split (`ConfigFileError` parse vs `ConfigError` validate); `Config::load` is the single entry point and the only builder of the resolved `Config`. A missing config file is recoverable when a root comes from elsewhere; a missing file behind `OCLI_CONFIG` is terminal (a typo signal, never masked).
- **D23 — `[vault]` section:** `features_dir` (default `notes/features`), `people_dir` (default `notes/people`) — the real vault nests under `notes/`.
- **D18 — `[frontmatter]` type table:** field → `string` (default) | `int` | `float` | `bool` | `olink` | `list<T>`; emission and validation per type; `ignore` list (default `relates-to`, `blocked-by`) is refused for writes; unknown type name or a type on a managed field → config-load error (fail-loud).
- **D19 — `[sections]` names:** defaults `Progress`, `Notes`, `Decisions`, `Open Questions`; one append primitive + thin sugar commands. Entry format `- YYYY-MM-DD HH:mm — text`; questions are `- [ ]` checkboxes, resolved by ticking in Obsidian. Notes stays unstructured scratch by design — promotion to Decisions/Questions is a human act.
- **D20 — branch pattern:** `[tickets] branch_pattern` (default `^(?<FeatureType>[A-Z]+)-(?<TicketNumber>\d+)`), all groups named, compiled at config load; invalid → config error. Policy: (a) ticket-needing commands error off-pattern, naming branch, pattern, and `--ticket`; (b) `new` *requires* a match — misfiled notes are the expensive mistake; (c) explicit key vs branch mismatch → warning, proceed. Key arguments are uppercase-normalized and validated.
- **D21 — value-map layers:** `--set` > key-argument-derived > branch named captures. Unnamed groups → config error; a template placeholder missing from all layers → render-time hard error naming it.
- **D22 — template statics:** `[template] path` + `[template.values]` scalars are the lowest value layer. Substitution is literal text — quoting is the template author's business (QuickAdd parity); `{{DATE:...}}`/`{{TIME:...}}` are renderer grammar, not config.

### Notes and writing

- **D8 — footer marker:** `<!-- ocli:footer -->` (authored by the user's template, never by ocli). Everything from the marker to EOF is untouchable; new sections insert at the marker line; no marker → append at EOF. Explicit convention beats a trailing-headings heuristic.
- **D11 — surgical write path:** read → parse to spans → apply edit operations (frontmatter splice unit = the field region, key line through continuation lines) → rebuild with untouched bytes identical → temp file in the same dir → atomic `rename`; re-read-and-retry if the source changed under us. `new` never rewrites existing files.
- **D12 — template rendering:** `new` renders the QuickAdd template as the single source of note shape. Explicit placeholder subset only (`{{VALUE:X}}`, `{{DATE:...}}`, `{{TIME:...}}`); anything unrecognized (e.g. Templater `<%...%>`) is a hard error naming the line. ocli generates no YAML itself.
- **D13 — repo identity:** last path segment of origin's URL, `.git` stripped (`git@host:org/repo.git` → `repo`); folder basename as fallback; `--repo` overrides both. `new` fills `repo: "[[<name>]]"` with no existence check; `list` filters to the current repo's tickets unless `--all-repos`.
- **D15 — owner inference:** superseded (2026-09) with the history layer, before any consumer shipped. Owner is set by hand — `fm owner "Name"` (D18's `olink` auto-wraps bare names). Revisit only if manual filling annoys in practice.
- **D16 — `fm`:** one general frontmatter setter over the D11 field-region edit; `status` is sugar adding vocabulary validation + `done:` sync; refuses managed fields (`status`/`done`/`Created`/`repo`) and `ignore`-listed ones; empty value empties the field, never removes it.
- **D17 — `Created`:** superseded (2026-09) — `Created` renders from the template's `{{DATE:...}}` at note-creation time, matching QuickAdd's own note-creation semantics (D12 parity). No git history, no epoch math.
- **D24 — `description`:** filled at creation via `--description`; hard error if the template lacks the field. `aliases` never touched. `list` shows it as-is — no title fallback; the id is the handle.
- **D25 — optional key:** `new` composes the ID from branch captures via `[tickets] id` (default `{FeatureType}-{TicketNumber}`); an explicit key argument is still accepted, with the D20c mismatch warning.
- **D26 — output contract:** human-readable only in v1; data → stdout, errors and warnings → stderr; exit 0/1. Write/append on a missing note → hard error naming branch/key/pattern with a "run `ocli new`" hint — never auto-create. Diagnostics via `tracing` to stderr (default `ocli=warn`, `RUST_LOG` respected); errors exit as color-eyre reports.
- **D29 — `open`:** `obsidian://open?path=...` URI via the OS opener (`xdg-open` on Linux). Read-only; the note must exist; current-ticket only, so it lands with Phase 5.

### Git

- **D14 — `gix` only:** no git subprocesses, no libgit2/C deps; pinned 0.87, feature-trimmed to `sha1` — ocli never touches the network; 0.x churn is contained to one adapter file.
- **D33 — adapter discipline:** gix types never escape; the public surface is `GitContext` + `context()`. (The D15 history walk — revwalk with hidden tips, mailmap — was removed with D15; its 0.x findings survive in git history and the old tests.)
- **D35 — context placement:** `Context` holds an inert `GitContext` snapshot derived tolerantly from the user's cwd — outside-a-repo is a *policy* error per command, not an environment failure; vault-only commands work without git. The live `GitRepo` exists only inside `context()`.

### Layout, errors, tests

- **D27 — library + thin binary:** `main.rs` is parsing/wiring/printing only; one-way dependency arrows — markdown is a leaf, git never imports vault, `commands/` is the only module seeing both git and vault.
- **D31 — file map** (amends D27's map, keeps its rules): `vault/{markdown, frontmatter, note, features}.rs`, `git/{mod, adapter}.rs` (adapter `pub(crate)`; `context()` lives in the facade), `config.rs`, `context.rs`, `commands/`. `ftypes` moves out of `config.rs` when the write path gives it a second consumer — moves happen at first real need, not on schedule.
- **D32 — error convention:** Display names the operation and path; causes travel via `#[source]` (color-eyre prints the chain once); distinct variants for kinds commands act on (`NoteNotFound` vs `IoError`); tests assert on causes via the `error_chain` helper.
- **D34 — git fixtures:** deterministic repositories built via the `git` CLI in temp dirs (test-setup only — the program never spawns git); tests exercise the same core entry points production uses.

## Remaining work

Phases end runnable. Landed phases keep one line; the details are in git history and the code.

- [x] **Phase 0 — hygiene:** scaffold, pinned deps (D28), tracing setup verified.
- [x] **Phase 1 — config:** full grammar/validate/resolve surface (D30); unresolved vault errors clearly.
- [x] **Phase 2 — read path:** markdown span locator, frontmatter projection, `Note::load`, features scan, `list` with status filter; fixture-vault integration tests (D6).
- [x] **Phase 3 — git facts core:** `context` over the adapter; branch→ticket mapping deliberately removed pending Q8. (The `first_commit_on_branch`/`branch_authors` history facts built here were later removed with D15/D17.)
- [x] **Phase 4 — git wiring:** tolerant `GitContext` snapshot in `Context`; `list` repo filtering deferred to Phase 5 (so `--all-repos` is currently a no-op).
- [ ] **Phase 5 — create path:** `new` renders the QuickAdd template (D12) with the value map (Q8, D21/D22) — `Created` comes from the template's `{{DATE}}` (D17); `repo` (D13) and `description` (D24) are post-render surgical fills; writes via `File::create_new`; branch checks (D20b); optional-key inference (D25). Also lands here: `list` repo filtering, `open` (D29).
- [ ] **Phase 6 — write path:** D11 machinery (field-region edits, multi-edit one-pass rebuild, atomic temp+rename); `status`, the append commands (D19), `fm` (D16); `ftypes` moves out of `config.rs`.
- [ ] **Phase 7 — cross-ticket views:** `questions` aggregates unchecked entries across `features/`; `--all` widens.
- [ ] **Phase 8 — polish:** clippy/fmt clean, shell completions, help text, `assert_cmd` binary-surface tests (exit codes, stdout/stderr contract — the D26 end-to-end tier).

## Open questions

- **Q4 — note types:** current notes carry `type: BCP`; v1 is features-only (assumed). Revisit if `list` must cover other folders.
- **Q6 — `estimate`/`sprint` fields:** leave alone in v1 (assumed); typed via D18 if ever managed.
- **Q8 — value-source pipeline** (decide at Phase 5, not before): template placeholders resolve from layered sources with D21/D22 precedence. Sketch: a `ValueMap` (`BTreeMap<String, String>` + per-name provenance, so argument-vs-branch conflicts can warn per D20c) merged once, consumed by a dumb renderer; `{{DATE}}`/`{{TIME}}` stay renderer grammar; the surgical fills (`repo`, `description`) stay a separate post-render step. The removed branch→ticket functions become the Branch layer's source adapter — their home falls out of this design.

## Learning log

Rust topics this project exercises, ticked off as encountered:

- [x] Ownership/borrowing review in real context (file IO lifetimes)
- [x] Error handling: `color-eyre`/`eyre` vs `thiserror`, `?`, custom error enums
- [x] Diagnostics with `tracing` (EnvFilter, span-aware events) — warnings/debug behind verbosity, warn-and-skip events in scan/load paths
- [x] `serde` data modeling with optional fields and enums
- [x] `clap` derive API and subcommands
- [x] Module layout for a multi-command binary
- [x] Regex, string parsing (branch names, frontmatter)
- [x] Integration testing with temp fixtures (`tempfile` or std)
- [x] Git access via `gix` (gitoxide): repo discovery, HEAD, remote URLs — revwalk with hidden tips and mailmap were exercised by the since-removed D15 history layer (the lessons survive in git history)