//! Integration tests for `ocli open` (D29): fixture vault + fixture repo
//! in temp dirs (never the real vault), through the public library API
//! only. The command's success side effect is handing a URI to the OS
//! opener — which must never launch a real application from a test — so
//! the URI bytes are pinned at the unit level ([`commands::open`]'s
//! `uri_for`) and these cover the refusal paths, all of which error
//! before the opener runs. The binary-surface tier (D26, exit codes and
//! stdout/stderr) is Phase 8.

mod common;

use common::TestVault;
use ocli::commands::open;

const ORIGIN: &str = "https://host/org/connected-module-item-api.git";

/// D35: outside a repository there is no git context — `open` infers the
/// ticket from the branch, so the degraded snapshot (branch `None`) is a
/// policy error, not a silent fallthrough.
#[test]
fn outside_a_repository_refuses_the_missing_branch() -> color_eyre::eyre::Result<()> {
    let vault = TestVault::fixture(&["open"])?;

    let err = open::run(&vault.context).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("could not find current branch"),
        "names the missing branch: {message}"
    );
    Ok(())
}

/// D20b: an off-pattern branch has no ticket — a hard error naming
/// branch and pattern, before any note lookup.
#[test]
fn off_pattern_branch_is_refused() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "main", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["open"], &repo)?;

    let err = open::run(&vault.context).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("main"), "names the branch: {message}");
    assert!(
        message.contains("FeatureType"),
        "names the pattern: {message}"
    );
    Ok(())
}

/// D29: the note must exist — `open` never creates; the error names the
/// path (D32), the "run ocli new" moment (D26).
#[test]
fn missing_note_names_the_path() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "NGP-500-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["open"], &repo)?;

    let err = open::run(&vault.context).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("could not find note at"),
        "names the failure: {message}"
    );
    let expected = vault
        .context
        .features_path()
        .join("NGP-500.md")
        .display()
        .to_string();
    assert!(
        message.contains(&expected),
        "names the note path: {message}"
    );
    Ok(())
}
