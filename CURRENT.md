# CURRENT — Phase 6: the write path

Working file for the live phase. Not a spec.

Rules for this file:

- Phase-local context and in-flight decisions live *here*, not in `PLAN.md` — PLAN changes only when a decision is durable.
- Decisions here are provisional while the phase is live; this file wins over PLAN for phase work. At phase close, durable outcomes are promoted into PLAN.md (new/amended D-entries) and everything else here is deleted — git history is the archive.
- Keep it short. Details go in code comments and commit messages.

## Where we are

- **Landed (slices 1–4):** `edit_note` (D11 atomic write + retry), `fm` (typed setter, D16/D18), `status` (vocab + `done:` sync, D37), `section` (config-driven read + append, D19). All four Phase-6 slices done — 167 tests, clippy clean.

## `section` — how it landed (D19)

- `ocli section <key>` reads the note's section; `ocli section <key> add "text"` appends.
- Sections are **entirely config-defined, no defaults**: `[sections] <key> = { heading = "## Line", format = "log" | "list" }` — the heading carries its own `#` level, `format` picks timestamped-log vs `- [ ]` checklist.
- `markdown::Section` now records heading `level` (the parser already counted `#`, now it's kept), so `## X` matches only an h2, `### X` only an h3.
- New sections insert at the footer marker (D8) or EOF; entries are single-line.

## Remaining before Phase 6 close-out

1. **Heading validation at config load** — a malformed `heading` (`#######`, no space, empty) is only caught when the `section` command resolves it; fold into `ConfigFile::validate` for fail-loud at load.
2. **Close-out** — promote the durable outcomes (D11's multi-edit simplification, D19's config-driven sections, the `section` command) into PLAN and reset this file; tick the Phase 6 checklist item.

## Decisions pending

- Read output: print the heading line verbatim plus its entries (current behavior) — no change expected.