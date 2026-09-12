//! Git adapter: the only module holding `gix` types. All methods return
//! plain data defined in the facade (`super`), so gix types never escape
//! (D14).

use std::path::{Path, PathBuf};

use gix::bstr::ByteSlice;

use super::{GitContext, GitError};

/// Wraps a discovered repository. The `gix::Repository` never escapes
/// (D14) — all methods return plain data.
pub struct GitRepo {
    repo: gix::Repository,
    start: PathBuf,
}

impl GitRepo {
    /// Discovers the repository containing `path`, walking upward.
    pub fn discover(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let start = path.as_ref().to_path_buf();
        let repo = gix::discover(path).map_err(|error| GitError::NotARepo {
            path: start.display().to_string(),
            error: Box::new(error),
        })?;
        Ok(Self { repo, start })
    }

    pub fn context(&self) -> Result<GitContext, GitError> {
        Ok(GitContext {
            branch: self.branch()?,
            repo_name: self.repo_name(),
        })
    }

    /// Short branch name; `None` when HEAD is detached (D20a policy).
    /// Private: the public read path is [`GitRepo::context`].
    pub fn branch(&self) -> Result<Option<String>, GitError> {
        let head = self.repo.head().map_err(|error| GitError::Branch {
            error: Box::new(error),
        })?;
        Ok(head
            .referent_name()
            .map(|name| name.shorten().to_str_lossy().into_owned()))
    }

    /// Repo identity (D13). Private: the public read path is
    /// [`GitRepo::context`]. Any failure to read `origin` counts as
    /// no-origin and falls back to the folder basename — the fallback is
    /// a documented last resort, not an error.
    pub fn repo_name(&self) -> String {
        let url = self
            .repo
            .find_remote("origin")
            .ok()
            .and_then(|remote| remote.url(gix::remote::Direction::Fetch).cloned());
        if let Some(name) = url
            .as_ref()
            .and_then(|u| repo_name_from_url(&u.to_string()))
        {
            return name;
        }
        // D13 fallback: the workdir folder basename (bare repos: git-dir
        // parent; never discoverable: the search dir; never: "unknown").
        self.repo
            .workdir()
            .or_else(|| self.repo.git_dir().parent())
            .or(Some(self.start.as_path()))
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_string())
    }

}

/// Normalizes an origin URL to a repo name (D13): the last path segment
/// with the `.git` suffix stripped. Handles scp-style
/// (`git@host:org/repo.git`) and URL (`https://host/org/repo.git`) forms.
/// `None` when the URL has no path — the caller then falls back to the
/// folder basename.
pub fn repo_name_from_url(url: &str) -> Option<String> {
    // Path component: after `scheme://host` for URLs, after the `:` for
    // scp-style. No path → `None` (caller falls back to folder basename).
    let tail = match url.split_once("//") {
        Some((_, rest)) => match rest.split_once('/') {
            Some((_, path)) => path,
            None => return None,
        },
        None => url.split_once(':').map(|(_, tail)| tail).unwrap_or(url),
    };
    let last = tail.trim_end_matches('/').rsplit('/').next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_name_from_url_covers_d13_forms() {
        assert_eq!(
            repo_name_from_url("git@host:org/connected-module-item-api.git"),
            Some("connected-module-item-api".to_string())
        );
        assert_eq!(
            repo_name_from_url("https://host/org/connected-module-item-api.git"),
            Some("connected-module-item-api".to_string())
        );
        assert_eq!(
            repo_name_from_url("https://host/org/repo"),
            Some("repo".to_string())
        );
        assert_eq!(
            repo_name_from_url("https://host/org/repo/"),
            Some("repo".to_string()),
            "trailing slash tolerated"
        );
        assert_eq!(
            repo_name_from_url("https://host"),
            None,
            "no path → no name → caller falls back to folder basename"
        );
    }
}
