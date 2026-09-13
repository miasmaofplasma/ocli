//! Git layer facade: the public git surface of the crate. The shared
//! vocabulary (`GitContext`, `GitError`) is defined here so the lower
//! module flows one way — `adapter` extracts facts from real repositories
//! — and gix types never escape (D14). No network features: ocli never
//! talks to remotes.

pub(crate) mod adapter;

use std::path::Path;

use thiserror::Error;

/// The git snapshot for a working directory. Discovery failure (not in a
/// repository) is the caller's policy decision, returned as an error.
pub fn context(repo_path: &Path) -> Result<GitContext, GitError> {
    adapter::GitRepo::discover(repo_path)?.context()
}

/// Everything the rest of the program may learn from a git repository.
#[derive(Debug, PartialEq)]
pub struct GitContext {
    /// Short branch name (e.g. `BCP-74043-fix-login`); `None` when HEAD is
    /// detached (D20a: the caller errors either way). An unborn branch is
    /// still `Some` — `new` must work before the first commit.
    pub branch: Option<String>,
    /// Repo identity (D13): last path segment of origin's fetch URL with
    /// the `.git` suffix stripped; falls back to the workdir folder
    /// basename when `origin` is missing or unreadable.
    pub repo_name: String,
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("no git repository found at or above {path}")]
    NotARepo {
        path: String,
        #[source]
        // gix facade errors are large enums — boxed to keep `Result<_, GitError>`
        // small (clippy result_large_err).
        error: Box<gix::discover::Error>,
    },
    #[error("could not read the current branch")]
    Branch {
        error: Box<gix::reference::find::existing::Error>,
    },
}
