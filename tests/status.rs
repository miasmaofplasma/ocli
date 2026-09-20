mod common;

use clap::Parser;
use common::TestVault;
use ocli::commands::status;

const ORIGIN: &str = "https://host/org/connected-module-item-api.git";

/// `Complete` sets the status and syncs `done: true`, leaving siblings and
/// the body byte-for-byte — the D11 splice plus the `done` sync (D36/PLAN:
/// `Complete` ⇔ `done: true`).
#[test]
fn complete_sets_status_and_syncs_done() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "BCP-74043-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["status", "Complete"], &repo)?;

    status::run(&vault.context)?;

    let text = std::fs::read_to_string(vault.context.features_path().join("BCP-74043.md"))?;
    assert!(text.contains("status: \"Complete\""), "got: {text}");
    assert!(text.contains("done: true"), "synced true: {text}");
    assert!(
        text.contains("description: Add SSO to the settings page"),
        "sibling kept: {text}"
    );
    assert!(text.contains("- kicked off"), "body kept: {text}");
    Ok(())
}

/// Any non-`Complete` status clears `done` — the fixture note starts
/// `done: true`, so `false` proves the write, not the absence of one.
#[test]
fn other_status_clears_done() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "NGP-1-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["status", "Backlog"], &repo)?;

    status::run(&vault.context)?;

    let text = std::fs::read_to_string(vault.context.features_path().join("NGP-1.md"))?;
    assert!(text.contains("status: \"Backlog\""), "got: {text}");
    assert!(text.contains("done: false"), "synced false: {text}");
    Ok(())
}

/// A near-miss status is rejected at argument parsing (exit 2), never
/// silently accepted as `Status::Unknown`.
#[test]
fn near_miss_status_is_rejected() {
    let err = ocli::cli::Cli::try_parse_from(["ocli", "status", "in progress"])
        .expect_err("near-miss status should be a usage error");
    assert!(err.to_string().contains("unknown status"), "got: {err}");
}