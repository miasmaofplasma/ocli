pub mod fm;
pub mod list;
pub mod new;
pub mod open;
pub mod status;

use std::path::PathBuf;

use crate::{
    context::Context,
    vault::{self, VaultError},
};

/// The current ticket from the git snapshot (D20b/D25): the branch check
/// plus the single regex pass whose `Captures` feed both the id
/// composition ([`vault::note_name`]) and the value map
/// ([`vault::capture_map`]).
///
/// Shared by `new` and `open` — this is the D27 boundary: `commands` is
/// the only module that sees both git (via `Context`) and vault.
///
/// Errors: no branch (outside a repository, degraded snapshot) →
/// `CurrentBranchNotFound`; off-pattern branch → `CouldNotParseBranchName`
/// naming branch and pattern.
pub fn current_ticket(context: &Context) -> Result<(String, regex::Captures<'_>), VaultError> {
    let git = context.git();
    let Some(branch) = git.branch.as_deref() else {
        return Err(VaultError::CurrentBranchNotFound);
    };
    let caps = vault::branch_captures(branch, &context.config().tickets.pattern)?;
    Ok((branch.to_string(), caps))
}

/// The note file for the current ticket: branch → id (D25), then the
/// existence check D29 requires — `open` never creates, a missing note
/// is the "run `ocli new`" moment (D26).
fn note_path(context: &Context) -> Result<PathBuf, VaultError> {
    let (branch, caps) = current_ticket(context)?;
    let tickets = &context.config().tickets;
    let name = vault::note_name(&branch, &tickets.pattern, &caps, &tickets.id)?;
    let path = context.features_path().join(format!("{name}.md"));

    // `is_file`, not exists: a directory at the note's path is not a
    // note. The metadata error (usually NotFound) is the real
    // `NoteNotFound` source — no synthesized failure for the common case.
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => Ok(path),
        Ok(_) => Err(VaultError::NoteNotFound {
            path: path.display().to_string(),
            error: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path exists but is not a file",
            ),
        }),
        Err(error) => Err(VaultError::NoteNotFound {
            path: path.display().to_string(),
            error,
        }),
    }
}
