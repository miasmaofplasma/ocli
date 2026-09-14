pub mod list;
pub mod new;
pub mod open;

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
