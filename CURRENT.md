# CURRENT — Phase 6: the write path

Working file for the live phase. Not a spec.

Rules for this file:

- Phase-local context and in-flight decisions live *here*, not in `PLAN.md` — PLAN changes only when a decision is durable.
- Decisions here are provisional while the phase is live; this file wins over PLAN for phase work. At phase close, durable outcomes are promoted into PLAN.md (new/amended D-entries) and everything else here is deleted — git history is the archive.
- Keep it short. Details go in code comments and commit messages.

## Goal

Phase 5 made ocli a reader + creator (`new`). Phase 6 makes it an *editor*: modify an existing note in place without corrupting it. The D11 write path — read → parse to spans → apply edit ops → rebuild with untouched bytes identical → atomic temp+rename → re-read-and-retry on conflict. First consumers: `fm`, `status`, the D19 append commands.

## Slices (each ends compiling and tested)

1. **D11 write core — edit ops + atomic write:** one edit-op type (a byte-range splice over the *original* text) applied in a single sorted pass — untouched bytes stay identical (the D11 contract); write via temp file in the same dir + `rename` (atomic; a reader never sees a half-written note); optimistic retry — re-read and re-apply if the note changed under us. Pure `rebuild(text, &ops)` unit-pinned for byte fidelity; write+retry integration-tested on a fixture note. `set_field`/`append_field` already supply the frontmatter field-region splice; this slice is the missing write half of D11.
2. **`fm` — the general setter (D16/D18):** `ocli fm <field> <value>` — coerce the value per the config `[frontmatter]` type table, refuse managed (`status`/`done`/`Created`/`repo`) and `ignore`-listed fields, strict `set_field` (`FieldNotFound` + closest-key suggestion), empty value empties the field; atomic write from s1. First real command over the machinery.
3. **`status` — vocabulary + `done:` sync (D16/D37):** `ocli status <Status>` — parse via `parse_strict` (near-miss → exit 2), set `status` to its Display form, sync `done` (`Complete` ⇒ `done: true`, else ⇒ `done: false`). Sets both fields via the s1 primitive directly — it *owns* them, bypassing `fm`'s managed-field refusal (that refusal is for the general setter).
4. **D19 appends — section entries:** the one genuinely new primitive — locate the configured `##` section (markdown `Document` sections + D8 footer marker) and insert `- YYYY-MM-DD HH:mm — text` after its last entry (`house_format`, D12); create the section at the footer marker (or EOF) when absent. Then the four thin commands `progress` / `note` / `decision` / `question` (questions = `- [ ]` checkboxes) over it.

## Already built — reused here, not rebuilt

- `set_field` / `append_field` / `set_or_append_field` + the field-region locator (`vault/frontmatter.rs`) — the frontmatter splice unit (D11).
- `markdown::parse` → `Document` (frontmatter/body/section/footer spans) — the byte-preserving parse (D27).
- `Field` typed emission (`ftypes.rs`) + the `[frontmatter]` type table (`config.rs`, D18).
- `house_format` / `DATE_FORMAT` (`vault/template.rs`) — the D19 entry timestamp.
- `Status` + `parse_strict` (`status.rs`, D37).

## Decisions pending (make each when its slice needs it, not before)

- Conflict detection for the retry: byte-compare vs. mtime; retry bound before a hard error.
- Where the edit-op list and write live: a new `vault/edit.rs` vs. extending `note.rs`.
- `fm` value coercion: quote/escape ownership per config type (mostly settled in `ftypes::Field`).
