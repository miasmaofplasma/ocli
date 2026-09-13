//! `ocli new` (D12/D25): render the QuickAdd template into a new feature
//! note. This module is the *only* orchestrator of the create path — the
//! vault modules are pure leaves (template.rs renders text, frontmatter.rs
//! edits regions), and every git/config/CLI decision happens here, in the
//! step order CURRENT.md defines.

use std::io::Write;
use std::path::PathBuf;

use chrono::Local;
use color_eyre::eyre::eyre;
use tracing::instrument;

use crate::{
    cli,
    context::Context,
    ftypes::Field,
    vault::{
        self, frontmatter, markdown,
        template::{FeatureTemplate, Source, ValueMap},
    },
};

/// The status every created note starts with (user decision 2026-09-12,
/// amending the "template encodes initial state" stance): work starts the
/// moment the note exists. Part of the status vocabulary
/// (`Backlog`/`In Progress`/`In Review`/`Complete`/`Blocked`) that
/// Phase 6's `status` command will validate — one constant until that
/// vocabulary gets a home.
pub const INITIAL_STATUS: &str = "In Progress";

/// Creates the note and returns its path — the command's data, which
/// `main` prints to stdout (D26: data → stdout, warnings → stderr).
#[instrument(skip(context))]
pub fn run(context: &Context) -> color_eyre::Result<PathBuf> {
    let cli::Command::New {
        key,
        description,
        set,
        repo,
    } = &context.cli().command
    else {
        return Err(eyre!("new command incorrectly called"));
    };

    // (s1, step 1–2) Branch check — the D20b hard require: `new` refuses
    // off-pattern branches because a misfiled note is the expensive
    // mistake. Unborn branches are fine: the name exists, and the
    // captures need no commit. One regex pass feeds both the value map
    // and the id composition.
    let git = context.git();
    let Some(branch) = git.branch.as_deref() else {
        return Err(vault::VaultError::CurrentBranchNotFound.into());
    };
    let tickets = &context.config().tickets;
    let caps = vault::branch_captures(branch, &tickets.pattern)?;

    // (s1, step 3) The id: composed from the captures; an explicit key
    // wins, with the D20c case-insensitive mismatch warning.
    let composed = vault::note_name(branch, &tickets.pattern, &caps, &tickets.id)?;
    let id = match key {
        Some(explicit) => {
            // The filename-safety gate is machinery, not convention — it
            // applies to the explicit key exactly as to the composed id:
            // `ocli new ../evil` must not escape features/.
            vault::ensure_safe_note_name(branch, explicit)?;
            if !explicit.eq_ignore_ascii_case(&composed) {
                tracing::warn!(
                    branch_id = %composed,
                    key = %explicit,
                    "explicit key differs from the branch-derived id; using the explicit key"
                );
            }
            explicit.clone()
        }
        None => composed,
    };

    // (s2, step 4) The value map — layers inserted in ascending
    // precedence, later inserts overwrite (D21/D22):
    // [template.values] < branch captures < key-derived < --set.
    let mut values = ValueMap::new();
    values.insert_all(Source::Template, context.config().template.values.clone());
    values.insert_all(Source::Branch, vault::capture_map(&tickets.pattern, &caps));
    if let Some(key) = key {
        // Key-derived: captures from matching the pattern against the
        // explicit key. A non-matching key contributes no layer — its
        // mismatch with the branch id already warned above.
        if let Some(key_caps) = tickets.pattern.captures(key) {
            values.insert_all(Source::Key, vault::capture_map(&tickets.pattern, &key_caps));
        }
    }
    for item in set {
        let malformed = || eyre!("--set expects NAME=VALUE, got {item:?}");
        let (name, value) = item.split_once('=').ok_or_else(malformed)?;
        if name.is_empty() {
            return Err(malformed());
        }
        values.insert(Source::Set, name, value);
    }

    // (s2, steps 5–7) Load + render. `Local::now()` is captured once and
    // threaded through, so every {{DATE}} in the note agrees with itself.
    let template_path = context
        .config()
        .vault
        .root
        .join(&context.config().template.path);
    let template = FeatureTemplate::load(&template_path)?;
    let now = Local::now();
    let text = template.render(&values, &now)?;
    // (s3, steps 8–9c) Fills: post-render surgical edits on the inner
    // frontmatter — `repo` always (D13, `--repo` overrides the git
    // snapshot's repo name), `description` only with `--description`
    // (D24), `status` always ([`INITIAL_STATUS`]; `done: false` already
    // matches — the note is not `Complete`). A missing field warns and
    // appends: the template is the drift source, not the user.
    let document = markdown::parse(&text);
    let fm_span = document
        .frontmatter()
        .ok_or_else(|| vault::VaultError::FrontmatterNotFound {
            path: template.path().display().to_string(),
        })?;
    let inner = frontmatter::inner_yaml(document.get(fm_span));
    let repo_name = repo
        .clone()
        .unwrap_or_else(|| git.repo_name.clone().unwrap_or_else(|| "unknown".into()));
    let inner = frontmatter::set_or_append_field(inner, "repo", Field::Olink(repo_name))?;
    let inner = match description {
        Some(desc) => {
            frontmatter::set_or_append_field(&inner, "description", Field::Str(desc.clone()))?
        }
        None => inner,
    };
    let inner =
        frontmatter::set_or_append_field(&inner, "status", Field::Str(INITIAL_STATUS.into()))?;
    let text = frontmatter::splice_inner(&text, fm_span, &inner);

    // (s4, step 10) Sanity check: the rendered frontmatter must parse as
    // YAML before it is allowed on disk.
    frontmatter::deserialize_frontmatter(&inner)?;

    // (s4, step 11) Write: refuse-to-overwrite via `create_new` (O_CREAT
    // | O_EXCL) — the syscall is the check, no pre-check race.
    let path = context.features_path().join(format!("{id}.md"));
    let mut file = std::fs::File::create_new(&path).map_err(|error| match error.kind() {
        std::io::ErrorKind::AlreadyExists => vault::VaultError::NoteAlreadyExists {
            path: path.display().to_string(),
            error,
        },
        _ => vault::VaultError::IoError {
            path: path.display().to_string(),
            error,
        },
    })?;
    file.write_all(text.as_bytes())
        .map_err(|error| vault::VaultError::IoError {
            path: path.display().to_string(),
            error,
        })?;

    Ok(path)
}
