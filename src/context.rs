//! The per-invocation environment (D27 boundary): resolved config, the
//! CLI invocation, and the git snapshot — validated against the
//! filesystem once, at construction. After `new`, a `Context` is inert
//! data — commands take `&Context` and never touch discovery or
//! validation.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::git::GitContext;
use crate::{cli::Cli, config::Config};

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("vault directory does not exist: {0}")]
    VaultDoesNotExist(PathBuf),
    #[error("features directory does not exist: {0}")]
    FeaturesDirMissing(PathBuf),
}

#[derive(Debug)]
pub struct Context {
    config: Config,
    cli: Cli,
    /// Git snapshot (D35 wiring): derived eagerly and *tolerantly* —
    /// being outside a repo is a policy error for specific commands
    /// (ticket inference), not an environment failure like a missing
    /// vault root, so it degrades to a defaulted snapshot here.
    git: GitContext,
}

impl Context {
    /// Validates runtime facts the config layer deliberately doesn't know
    /// about (directories can appear or vanish between config load and
    /// command execution — checking them here checks *now*). Only facts
    /// every command depends on belong here: the vault root, the features
    /// directory that all ticket operations route through (list scans
    /// it, new writes into it, open resolves through it), and the git
    /// snapshot derived from `git_start` (the user's cwd — work repos and
    /// the vault are unrelated locations).
    pub fn new(config: Config, cli: Cli, git_start: &Path) -> Result<Self, ContextError> {
        // is_dir, not exists: a file at the configured root would pass an
        // existence check and then blow up on the first directory read.
        if !config.vault.root.is_dir() {
            return Err(ContextError::VaultDoesNotExist(config.vault.root.clone()));
        }

        let features_path = config.vault.root.join(&config.vault.features_dir);
        if !features_path.is_dir() {
            return Err(ContextError::FeaturesDirMissing(features_path));
        }

        let git = crate::git::context(git_start).unwrap_or_else(|error| {
            // Not in a repo: not this function's error — ticket commands
            // raise the "must be on a ticket branch" policy error later,
            // with the command context to explain it.
            tracing::debug!(
                path = %git_start.display(),
                error = %error,
                "no git repository; continuing without git context"
            );
            // No repo → no identity: `repo_name: None` is the marker
            // `list` uses to skip repo filtering (an invented identity
            // would silently hide every note).
            GitContext {
                branch: None,
                repo_name: None,
            }
        });

        Ok(Self { config, cli, git })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn cli(&self) -> &Cli {
        &self.cli
    }

    /// The git snapshot (inert data, per D35): branch and repo identity.
    pub fn git(&self) -> &GitContext {
        &self.git
    }

    pub fn features_path(&self) -> PathBuf {
        self.config.vault.root.join(&self.config.vault.features_dir)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use clap::Parser;

    use regex::Regex;

    use super::*;
    use crate::config::{FrontmatterCfg, ResolvedVault, TemplateCfg, Tickets};

    /// A minimal valid `Config` pointing at `root` — built directly, since
    /// config validation is deliberately filesystem-free.
    fn config_with_root(root: PathBuf) -> Config {
        Config {
            vault: ResolvedVault {
                root,
                features_dir: PathBuf::from("notes/features"),
                people_dir: PathBuf::from("notes/people"),
            },
            frontmatter: FrontmatterCfg {
                ignore: Vec::new(),
                types: BTreeMap::new(),
            },
            template: TemplateCfg::default(),
            tickets: Tickets {
                pattern: Regex::new(r"^(?<Key>[A-Z]+)-(?<Num>\d+)").unwrap(),
                id: "{Key}-{Num}".to_string(),
            },
            sections: BTreeMap::new(),
        }
    }

    fn cli() -> Cli {
        Cli::parse_from(["ocli", "list"])
    }

    #[test]
    fn accepts_existing_vault_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("notes/features")).unwrap();
        let ctx = Context::new(
            config_with_root(dir.path().to_path_buf()),
            cli(),
            dir.path(),
        );
        assert!(ctx.is_ok());
    }

    #[test]
    fn rejects_missing_vault_directory() {
        let config = config_with_root(PathBuf::from("/definitely/not/a/vault"));
        let err = Context::new(config, cli(), Path::new("/tmp")).unwrap_err();
        assert!(matches!(err, ContextError::VaultDoesNotExist(_)));
        assert!(
            err.to_string().contains("/definitely/not/a/vault"),
            "error should name the path: {err}"
        );
    }

    #[test]
    fn rejects_vault_root_that_is_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not-a-dir");
        std::fs::write(&file, "x").unwrap();
        let err = Context::new(config_with_root(file), cli(), Path::new("/tmp")).unwrap_err();
        assert!(matches!(err, ContextError::VaultDoesNotExist(_)));
    }

    #[test]
    fn rejects_missing_features_dir() {
        let dir = tempfile::tempdir().unwrap();
        // Root exists; notes/features was never created.
        let err = Context::new(
            config_with_root(dir.path().to_path_buf()),
            cli(),
            dir.path(),
        )
        .unwrap_err();
        assert!(matches!(err, ContextError::FeaturesDirMissing(_)));
        assert!(
            err.to_string().contains("notes/features"),
            "error should name the configured path: {err}"
        );
    }

    #[test]
    fn rejects_features_dir_that_is_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes/features");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "x").unwrap();
        let err = Context::new(
            config_with_root(dir.path().to_path_buf()),
            cli(),
            dir.path(),
        )
        .unwrap_err();
        assert!(matches!(err, ContextError::FeaturesDirMissing(_)));
    }

    #[test]
    fn can_resolve_features_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("notes/features")).unwrap();
        let ctx = Context::new(
            config_with_root(dir.path().to_path_buf()),
            cli(),
            dir.path(),
        )
        .unwrap();
        assert_eq!(ctx.features_path(), dir.path().join("notes/features"))
    }

    #[test]
    fn git_snapshot_defaults_outside_a_repo() {
        // The vault fixture dir is not a repo: discovery fails, snapshot
        // degrades (branch None, folder-name repo) — and construction
        // *succeeds*, because vault-only commands must work without git.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("notes/features")).unwrap();
        let ctx = Context::new(
            config_with_root(dir.path().to_path_buf()),
            cli(),
            dir.path(),
        )
        .unwrap();

        let git = ctx.git();
        assert_eq!(git.branch, None);
        assert_eq!(
            git.repo_name, None,
            "no repo → no identity: list must not filter by an invented one"
        );
    }

    #[test]
    fn git_snapshot_derives_from_start_dir_repo() {
        // A repo at the *start dir*, with the vault elsewhere — proving
        // work-repo and vault locations are independent.
        let temp = tempfile::tempdir().unwrap();
        let repo_dir = temp.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::create_dir_all(temp.path().join("vault/notes/features")).unwrap();
        std::process::Command::new("git")
            .args(["init", "-q", "-b", "BCP-74043-work"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::fs::write(repo_dir.join("f.txt"), "x").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@e.c",
                "commit",
                "-q",
                "-m",
                "c",
            ])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        let ctx = Context::new(
            config_with_root(temp.path().join("vault")),
            cli(),
            &repo_dir,
        )
        .unwrap();
        let git = ctx.git();
        assert_eq!(git.branch.as_deref(), Some("BCP-74043-work"));
        assert_eq!(
            git.repo_name,
            Some("repo".to_string()),
            "no origin → folder-basename fallback"
        );
    }
}
