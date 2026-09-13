//! Shared harness for integration tests: copies the checked-in fixture
//! vault into a fresh temp dir per test (never the real vault) and builds
//! a validated `Context` via the programmatic constructor, plus git
//! fixture helpers. Each `tests/*.rs` target uses whichever parts it
//! needs — hence the module-wide dead-code allowance.
#![allow(dead_code)]

use clap::Parser;
use color_eyre::eyre::WrapErr;
use ocli::cli::Cli;
use ocli::config::Config;
use ocli::context::Context;
use std::path::Path;

pub struct TestVault {
    /// Holds the temp dir open; the context's paths die with it.
    _temp: tempfile::TempDir,
    pub context: Context,
}

impl TestVault {
    /// Fixture vault copied to a fresh temp dir. `args` are the CLI words
    /// after the program name, e.g. `&["list", "--status", "In Progress"]`.
    pub fn fixture(args: &[&str]) -> color_eyre::eyre::Result<Self> {
        let temp = tempfile::tempdir().wrap_err("creating temp dir")?;
        copy_dir(&fixture_root(), temp.path())?;
        Self::build(temp, args)
    }

    /// Fixture vault + git discovery from `repo_dir` — for commands that
    /// need branch/repo identity (`new`). `repo_dir` must outlive the
    /// returned vault (keep its temp dir in the test).
    pub fn fixture_with_git(
        args: &[&str],
        repo_dir: &std::path::Path,
    ) -> color_eyre::eyre::Result<Self> {
        let temp = tempfile::tempdir().wrap_err("creating temp dir")?;
        copy_dir(&fixture_root(), temp.path())?;
        Self::build_with(temp, args, repo_dir)
    }

    /// An empty vault: root and `notes/features` exist, no notes inside.
    pub fn empty(args: &[&str]) -> color_eyre::eyre::Result<Self> {
        let temp = tempfile::tempdir().wrap_err("creating temp dir")?;
        std::fs::create_dir_all(temp.path().join("notes/features"))?;
        Self::build(temp, args)
    }

    fn build(temp: tempfile::TempDir, args: &[&str]) -> color_eyre::eyre::Result<Self> {
        Self::build_with(
            temp,
            args,
            // Git discovery starts outside any repo (a fresh temp path) —
            // vault fixtures test the no-git degraded path; tests that
            // need a repo build one via the git helpers and pass it here.
            std::path::Path::new("/nonexistent-no-repo"),
        )
    }

    fn build_with(
        temp: tempfile::TempDir,
        args: &[&str],
        git_start: &std::path::Path,
    ) -> color_eyre::eyre::Result<Self> {
        let config = Config::for_vault(temp.path().to_path_buf())?;
        let cli = Cli::parse_from(std::iter::once("ocli").chain(args.iter().copied()));
        let context =
            Context::new(config, cli, git_start).map_err(color_eyre::eyre::Report::new)?;
        Ok(Self {
            _temp: temp,
            context,
        })
    }
}

fn fixture_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vault")
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Git fixture helpers

/// Runs a `git` subcommand in `dir`, failing the test on error. Test-setup
/// only: the ocli program itself never spawns git (the plan's rule) — but
/// building *fixture repositories* deterministically is exactly what the
/// CLI is best at. Requires the `git` binary in the test environment.
pub fn run_git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .env("GIT_AUTHOR_NAME", "Ada Lovelace")
        .env("GIT_AUTHOR_EMAIL", "ada@example.com")
        .env("GIT_COMMITTER_NAME", "Ada Lovelace")
        .env("GIT_COMMITTER_EMAIL", "ada@example.com")
        .current_dir(dir)
        .output()
        .expect("git binary is required for git-fixture tests");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Creates an empty commit with an explicit author identity and date, so
/// fixtures are byte-deterministic (dates pinned, no clock reads).
pub fn commit(dir: &Path, name: &str, email: &str, date: &str, msg: &str) {
    let out = std::process::Command::new("git")
        .args(["commit", "-q", "--allow-empty", "-m", msg])
        .env("GIT_AUTHOR_NAME", name)
        .env("GIT_AUTHOR_EMAIL", email)
        .env("GIT_COMMITTER_NAME", name)
        .env("GIT_COMMITTER_EMAIL", email)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .current_dir(dir)
        .output()
        .expect("git binary is required for git-fixture tests");
    assert!(
        out.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The standard scenario most tests will want: a repo on a ticket branch,
/// one commit, with or without an `origin` remote. Returns the repo path.
pub fn fixture_repo(temp: &Path, branch: &str, origin_url: Option<&str>) -> std::path::PathBuf {
    let repo_dir = temp.join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();
    run_git(&repo_dir, &["init", "-q", "-b", branch]);
    commit(
        &repo_dir,
        "Ada Lovelace",
        "ada@example.com",
        "2026-01-01T00:00:00+00:00",
        "c1",
    );
    if let Some(url) = origin_url {
        run_git(&repo_dir, &["remote", "add", "origin", url]);
    }
    repo_dir
}
