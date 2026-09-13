//! Integration tests for the git layer (`ocli::git`): each test builds a
//! deterministic fixture repository in a temp dir via the shared `git`-CLI
//! helpers and asserts on the facade's output. The `git` binary is
//! test-setup only — the ocli program never spawns git. Tests go through
//! `context` (the same path production code uses), never the
//! crate-private adapter.
mod common;

use ocli::git::context;

#[test]
fn discovers_branch_and_repo_name_from_origin() {
    let temp = tempfile::tempdir().unwrap();
    let repo_dir = common::fixture_repo(
        temp.path(),
        "BCP-74043-fix-login",
        Some("https://host/org/connected-module-item-api.git"),
    );

    let ctx = context(&repo_dir).unwrap();
    assert_eq!(ctx.branch.as_deref(), Some("BCP-74043-fix-login"));
    assert_eq!(
        ctx.repo_name.as_deref(),
        Some("connected-module-item-api"),
        "origin's URL names the repo (D13)"
    );
}

#[test]
fn repo_name_falls_back_to_folder_basename() {
    let temp = tempfile::tempdir().unwrap();
    let repo_dir = temp.path().join("vault-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    common::run_git(&repo_dir, &["init", "-q", "-b", "main"]);
    assert_eq!(
        context(&repo_dir).unwrap().repo_name.as_deref(),
        Some("vault-repo")
    );
}

#[test]
fn unborn_branch_keeps_its_name() {
    let temp = tempfile::tempdir().unwrap();
    let repo_dir = temp.path().join("repo");
    std::fs::create_dir(&repo_dir).unwrap();
    common::run_git(&repo_dir, &["init", "-q", "-b", "BCP-1-still-empty"]);

    let ctx = context(&repo_dir).unwrap();
    assert_eq!(
        ctx.branch.as_deref(),
        Some("BCP-1-still-empty"),
        "new must work before the first commit"
    );
}

#[test]
fn detached_head_yields_no_branch() {
    let temp = tempfile::tempdir().unwrap();
    let repo_dir = common::fixture_repo(temp.path(), "main", None);
    common::run_git(&repo_dir, &["checkout", "-q", "--detach"]);

    assert_eq!(context(&repo_dir).unwrap().branch, None);
}
