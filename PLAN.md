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
- Git is the context. The current branch must match a configurable regex whose named captures (e.g. `FeatureType`, `TicketNumber`) feed the template; the origin remote maps to the `repo` frontmatter field. Commands infer the ticket from where you are, with an explicit key override (D25).
- Read anything; write only owned frontmatter fields and CLI-owned `##` sections. Never touch user prose, wikilinks, Meta Bind blocks, or anything past the footer marker (D8).
- Status vocabulary: `Backlog`, `In Progress`, `In Review`, `Complete`, `Blocked`; `Complete` ⇔ `done: true` kept in sync.
- v1 command set: `new`, `list`, `status`, `fm`, `section`, `open` (README has the full table).

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
- **D28 — dependency set** (supersedes D3): `clap` (derive), `serde`, `toml`, `directories`, `regex`, `chrono 0.4` (feature-trimmed: `clock`, `std`; one house format via integer accessors — added Phase 5 for `{{DATE}}`), `thiserror`, `color-eyre`, `tracing` + `tracing-subscriber` (env-filter), `yaml_serde 0.10`, `gix 0.87` (pinned, D14), `open 5` + `percent-encoding 2` (the OS URI opener and the `obsidian://` path encoding — Phase 5's `open`); dev `tempfile`. Deliberately not adopted: `tokio`, `async-trait`, `sqlx` — nothing here waits (D1/D10).

### Config

- **D7 — config location:** `~/.config/ocli/config.toml` via `directories` (`ProjectDirs`); `OCLI_CONFIG` overrides.
- **D30 — vault root precedence:** `--vault` > `OCLI_VAULT` > `[vault] root`. Grammar/policy error split (`ConfigFileError` parse vs `ConfigError` validate); `Config::load` is the single entry point and the only builder of the resolved `Config`. A missing config file is recoverable when a root comes from elsewhere; a missing file behind `OCLI_CONFIG` is terminal (a typo signal, never masked).
- **D23 — `[vault]` section:** `features_dir` (default `notes/features`), `people_dir` (default `notes/people`) — the real vault nests under `notes/`.
- **D18 — `[frontmatter]` type table:** field → `string` (default) | `int` | `float` | `bool` | `olink` | `list<T>`; emission and validation per type; `ignore` list (default `relates-to`, `blocked-by`) is refused for writes; unknown type name or a type on a managed field → config-load error (fail-loud).
- **D19 — `[sections]` (amended 2026-09):** the section table *is* the command vocabulary — `ocli section <key>` reads the current note's section; `ocli section <key> add "text"` appends. Each entry declares the full heading line (marker included, so any `#` level) plus a format — `log` (`- YYYY-MM-DD HH:mm — text`) or `list` (`- [ ] text`). No default sections — every section is user-declared. New sections insert at the footer marker (D8) or EOF. Drops the four sugar commands (`progress`/`note`/`decision`/`question`) and the cross-ticket `questions` aggregate — section commands are per-note only.
- **D20 — branch pattern:** `[tickets] branch_pattern` (default `^(?<FeatureType>[A-Z]+)-(?<TicketNumber>\d+)`), all groups named, compiled at config load; invalid → config error. Policy: (a) ticket-needing commands error off-pattern, naming branch and pattern; (b) `new` *requires* a match — misfiled notes are the expensive mistake; (c) explicit key vs branch-derived id mismatch → case-insensitive comparison, warning, proceed (revised 2026-09: no case-folding of arguments). Composed IDs have **no format rule** (revised 2026-09: the team's pattern + id template own the shape; the old uppercase/`[A-Z0-9]+-\d+` rules were argument-path rules) — two neutral gates instead: substitution integrity (a surviving `{...}` errors, showing the mangled composition) and filename safety (non-empty, no leading `.`, no `/`/NUL/newline — the note must be one visible file in `features/`; machinery, not convention, so it applies to the explicit key exactly as to the composed id).
- **D21 — value-map layers:** `--set` > key-argument-derived > branch named captures > `[template.values]` (D22). Provenance travels with each entry (`ValueMap`: `BTreeMap<String, (String, Source)>`), so conflicts between layers are attributable. Unnamed groups → config error; a template placeholder missing from all layers → render-time hard error naming it.
- **D22 — template statics:** `[template] path` + `[template.values]` scalars are the lowest value layer. Substitution is literal text — quoting is the template author's business (QuickAdd parity); date formatting is renderer grammar, not config (D12).

### Notes and writing

- **D8 — footer marker:** `<!-- ocli:footer -->` (authored by the user's template, never by ocli). Everything from the marker to EOF is untouchable; new sections insert at the marker line; no marker → append at EOF. Explicit convention beats a trailing-headings heuristic.
- **D11 — surgical write path:** read → parse to spans → apply edit operations (frontmatter splice unit = the field region, key line through continuation lines) → rebuild with untouched bytes identical → temp file in the same dir → atomic `rename`; re-read-and-retry if the source changed under us. `new` never rewrites existing files.
- **D12 — template rendering (amended 2026-09, Q8 resolved):** `new` renders the QuickAdd template as the single source of note shape. The grammar is exactly `{{VALUE:name}}` and `{{DATE:YYYY-MM-DD HH:mm}}` — the date argument is one house format, **exact-matched, never parsed**; `{{TIME:...}}` was dropped (audit found none); anything else (Templater `<%...%>`, unknown constructs) hard-errors naming the line. ocli generates no YAML itself. The pipeline is **substitute on text, validate after, never serialize**: one scan pass classifies placeholders, substitution rebuilds the text with untouched bytes verbatim, and the rendered frontmatter is only sanity-parsed as YAML afterwards. Rejected on evidence: parse→mutate→serialize via yaml_serde rewrites quote style/empty-field forms on real templates and passes unknown placeholders silently.
- **D13 — repo identity (amended 2026-09):** last path segment of origin's URL, `.git` stripped (`git@host:org/repo.git` → `repo`); folder basename as fallback inside a real repo; `--repo` overrides both. `new` fills `repo: "[[<name>]]"` with no existence check; `list` filters to the current repo's tickets unless `--all-repos`. Outside a repository there is **no identity** (`GitContext.repo_name: None`) — `list` then skips the filter and the whole vault lists; an invented name would silently hide every note. Matching and display normalize the note's `repo` field by stripping the olink wrapper — `[[name]]` and bare `name` are the same repo; alias-form `[[path|alias]]` does not match (v1).
- **D15 — owner inference:** superseded (2026-09) with the history layer, before any consumer shipped. Owner is set by hand — `fm owner "Name"` (D18's `olink` auto-wraps bare names). Revisit only if manual filling annoys in practice.
- **D16 — `fm`:** one general frontmatter setter over the D11 field-region edit; `status` is sugar adding vocabulary validation + `done:` sync; empty value empties the field, never removes it. Refuses `ignore`-listed fields (D5: they are other tooling's — writing them is data loss). Managed-field refusal **dropped** (revised 2026-09, user decision): `owner` is set by hand (D15 — refusing `fm owner` contradicted it), and `status`/`repo`/`done`/`Created` are writable raw; the `done:` sync lives in `ocli status`, which `fm` does not perform. (Revised 2026-09, typo hazard: the shared primitive is **strict** — `set_field` errors `FieldNotFound` with a closest-key suggestion, so a misspelled field name never becomes a junk field; `append_field` is the deliberate add, and `fm` never appends.)
- **D17 — `Created`:** superseded (2026-09) — `Created` renders from the template's `{{DATE:...}}` at note-creation time, matching QuickAdd's own note-creation semantics (D12 parity). No git history, no epoch math.
- **D24 — `description`:** filled at creation via `--description`; a template that lacks the field **warns and appends** (revised 2026-09 from hard error — appending is data-adding and the warning keeps template drift visible on every creation; see D36). `aliases` never touched. `list` shows it as-is — no title fallback; the id is the handle.
- **D25 — optional key:** `new` composes the ID from branch captures via `[tickets] id` (default `{FeatureType}-{TicketNumber}`); an explicit key argument is still accepted, with the D20c mismatch warning. The filename-safety gate (D20) applies to the explicit key exactly as to the composed id.
- **D36 — creation fills and initial status (2026-09):** `new` post-renders three surgical fills on the inner frontmatter — `repo` (D13), `description` (D24, only with `--description`), and `status: "In Progress"` **always** (user decision: work starts the moment the note exists; amends the earlier "template encodes initial state" stance — the real template ships `status:` empty precisely because ocli fills it). All three share one contract: `set_or_append_field` replaces in place, warns and appends when the template lacks the field. `done:` stays template-owned — the initial status is consistent with `done: false`, so no sync. If a template ever pre-seeds a *different* non-empty status, the fill overwrites it (recorded nuance; the refinement "fill only when empty" exists if it ever matters).
- **D26 — output contract:** human-readable only in v1; data → stdout, errors and warnings → stderr; exit 0/1. Write/append on a missing note → hard error naming branch/key/pattern with a "run `ocli new`" hint — never auto-create. Diagnostics via `tracing` to stderr (default `ocli=warn`, `RUST_LOG` respected); errors exit as color-eyre reports.
- **D29 — `open` (landed 2026-09):** `obsidian://open?path=...` with the note's *absolute* path — Obsidian resolves it against "the most specific vault which contains" it, so no vault name is needed. The whole path is percent-encoded (`percent-encoding`'s `NON_ALPHANUMERIC`; Obsidian's docs require it — an unencoded `#` becomes a heading/block jump instead of path). Hand-off via the `open` crate (`xdg-open` on Linux); the launcher's exit status is the only failure signal — success never means "note on screen". Read-only; note-must-exist via `fs::metadata` raising `NoteNotFound`; current-ticket only. The URI construction is a pure `uri_for` (unit-pinned bytes); the opener call is the only side effect, so refusal-path tests never launch anything.
- **D37 — status vocabulary (landed 2026-09, early for Phase 6):** `status.rs` is the single home — `Status` = the five canonical variants (D16) plus `Unknown(String)`, the read-side catch-all. Direction split: the *read* path is faithful — frontmatter `status` is `Option<Status>`, and an unrecognized word deserializes to `Unknown`, so a hand-written `status: Doing` loads and `list` shows it verbatim (never a load failure; "read anything", D8). The *input* path is strict — `FromStr` is infallible (`Err = Infallible`: every string maps, unknown → `Unknown`), and the CLI parses via `parse_strict` (→ `UnknownStatusError`, exit 2), so `--status` rejects a near-miss like `in progress` instead of silently matching nothing. `is_unknown()` gates the strict boundary; Phase 6's `status`/`fm` writes reuse `parse_strict`.

### Git

- **D14 — `gix` only:** no git subprocesses, no libgit2/C deps; pinned 0.87, feature-trimmed to `sha1` — ocli never touches the network; 0.x churn is contained to one adapter file.
- **D33 — adapter discipline:** gix types never escape; the public surface is `GitContext` + `context()`. (The D15 history walk — revwalk with hidden tips, mailmap — was removed with D15; its 0.x findings survive in git history and the old tests.)
- **D35 — context placement:** `Context` holds an inert `GitContext` snapshot derived tolerantly from the user's cwd — outside-a-repo is a *policy* error per command, not an environment failure; vault-only commands work without git. The live `GitRepo` exists only inside `context()`.

### Layout, errors, tests

- **D27 — library + thin binary:** `main.rs` is parsing/wiring/printing only; one-way dependency arrows — markdown is a leaf, git never imports vault, `commands/` is the only module seeing both git and vault.
- **D31 — file map** (amends D27's map, keeps its rules): `vault/{markdown, frontmatter, note, features, template}.rs`, `ftypes.rs` (field-type vocabulary: config's `FieldType` + the write path's emission `Field` — moved out of `config.rs` 2026-09 when the write path became its second consumer, per the first-real-need rule), `git/{mod, adapter}.rs` (adapter `pub(crate)`; `context()` lives in the facade), `config.rs`, `context.rs`, `commands/{list, new}.rs`.
- **D32 — error convention:** Display names the operation and path; causes travel via `#[source]` (color-eyre prints the chain once); distinct variants for kinds commands act on (`NoteNotFound` vs `IoError`); tests assert on causes via the `error_chain` helper.
- **D34 — git fixtures:** deterministic repositories built via the `git` CLI in temp dirs (test-setup only — the program never spawns git); tests exercise the same core entry points production uses.

## Remaining work

Phases end runnable. Landed phases keep one line; the details are in git history and the code.

- [x] **Phase 0 — hygiene:** scaffold, pinned deps (D28), tracing setup verified.
- [x] **Phase 1 — config:** full grammar/validate/resolve surface (D30); unresolved vault errors clearly.
- [x] **Phase 2 — read path:** markdown span locator, frontmatter projection, `Note::load`, features scan, `list` with status filter; fixture-vault integration tests (D6).
- [x] **Phase 3 — git facts core:** `context` over the adapter; branch→ticket mapping deliberately removed pending Q8. (The `first_commit_on_branch`/`branch_authors` history facts built here were later removed with D15/D17.)
- [x] **Phase 5 — create path (landed 2026-09):** `new` renders the QuickAdd template (D12) with the value map (D21/D22) — `Created` comes from the template's `{{DATE}}` (D17); `repo` (D13), `description` (D24), and `status: "In Progress"` (D36) are post-render surgical fills; writes via `File::create_new`; branch checks (D20b); optional-key inference (D25). Also landed here: `list` repo filtering (D13) and `open` (D29).
- [ ] **Phase 6 — write path:** D11 machinery (field-region edits, multi-edit one-pass rebuild, atomic temp+rename); `status`, `section` (D19), `fm` (D16). (`ftypes` — D31 — and the `Status` vocabulary — D37 — already extracted.)
- [ ] **Phase 7 — polish:** clippy/fmt clean, shell completions, help text, `assert_cmd` binary-surface tests (exit codes, stdout/stderr contract — the D26 end-to-end tier).

## Open questions

- **Q4 — note types:** current notes carry `type: BCP`; v1 is features-only (assumed). Revisit if `list` must cover other folders.
- **Q6 — `estimate`/`sprint` fields:** leave alone in v1 (assumed); typed via D18 if ever managed.
*(Q8 resolved 2026-09: the value-source pipeline is D12/D21/D22 — substitute-on-text renderer with a provenance-carrying `ValueMap`; removed here at promotion.)*

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
- [x] Time handling without strftime pitfalls: chrono feature-trimming (`clock`/`std`), integer accessors + std padding (doubled strftime specifiers are literals — verified empirically)
- [x] Byte-preserving text editing: span-located substitution and region splices over raw bytes, `File::create_new` (`O_CREAT|O_EXCL`) for refuse-to-overwrite writes
- [x] Layered value resolution with provenance (`BTreeMap<String, (String, Source)>` — later inserts overwrite, the merge *is* the precedence)
- [x] Infallible `FromStr` + a catch-all variant for faithful reads (lenient read / strict input split via `parse_strict`)