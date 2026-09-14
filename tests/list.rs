//! Integration tests for `ocli list` (D6): every test runs against a copy
//! of the checked-in fixture vault, through the public library API only.
//! Repo filtering (D13/Phase 5) is exercised with a fixture repo: the
//! fixture notes BCP-74043 and NGP-1 carry `repo:
//! connected-module-item-api`; `no-fm.md` carries none.

mod common;

use clap::Parser;
use common::TestVault;
use ocli::commands::list;
use ocli::status::Status;

const ORIGIN: &str = "https://host/org/connected-module-item-api.git";

#[test]
fn lists_fixture_notes_skipping_malformed() -> color_eyre::eyre::Result<()> {
    let vault = TestVault::fixture(&["list"])?;
    let rows = list::run(&vault.context)?;

    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["BCP-74043", "NGP-1", "no-fm"],
        "scan order preserved; broken.md skipped; no git context → no repo \
         filter, so the whole vault lists"
    );

    let bcp = &rows[0];
    assert_eq!(
        bcp.description.as_deref(),
        Some("Add SSO to the settings page")
    );
    assert_eq!(bcp.status, Some(Status::InProgress));
    assert_eq!(bcp.repo.as_deref(), Some("connected-module-item-api"));

    let ngp = &rows[1];
    assert_eq!(
        ngp.description, None,
        "no title fallback (D24): an empty description stays empty"
    );
    assert_eq!(ngp.status, Some(Status::Complete));

    Ok(())
}

#[test]
fn status_filter_matches_exactly() -> color_eyre::eyre::Result<()> {
    let vault = TestVault::fixture(&["list", "--status", "In Progress"])?;
    let rows = list::run(&vault.context)?;

    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["BCP-74043"]);

    Ok(())
}

#[test]
fn unknown_status_arg_is_rejected() {
    // `--status` is a typed Status now: a near-miss no longer silently
    // yields an empty list — clap refuses it before any note is read.
    let err = ocli::cli::Cli::try_parse_from(["ocli", "list", "--status", "in progresss"])
        .expect_err("near-miss status should be a usage error");
    assert!(err.to_string().contains("unknown status"), "got: {err}");
}

#[test]
fn empty_vault_yields_no_rows() -> color_eyre::eyre::Result<()> {
    let vault = TestVault::empty(&["list"])?;
    let rows = list::run(&vault.context)?;

    assert!(
        rows.is_empty(),
        "a ticket-less vault is a valid state, not an error"
    );

    Ok(())
}

/// D13: the default listing is the current repo's tickets only. Both
/// fixture notes name `connected-module-item-api`; `no-fm.md` carries no
/// repo field at all — an unattributed note belongs to no repo, so it
/// stays hidden under the filter.
#[test]
fn lists_only_the_current_repo_by_default() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "BCP-74043-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["list"], &repo)?;

    let rows = list::run(&vault.context)?;
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["BCP-74043", "NGP-1"],
        "no-fm.md has no repo field — unattributed notes stay hidden"
    );

    Ok(())
}

#[test]
fn all_repos_widens_past_the_repo_filter() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "BCP-74043-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["list", "--all-repos"], &repo)?;

    let rows = list::run(&vault.context)?;
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["BCP-74043", "NGP-1", "no-fm"],
        "--all-repos lists every note, attributed or not"
    );

    Ok(())
}

#[test]
fn other_repos_tickets_are_hidden() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(
        repo_temp.path(),
        "BCP-74043-fix-login",
        Some("https://host/org/some-other-repo.git"),
    );
    let vault = TestVault::fixture_with_git(&["list"], &repo)?;

    let rows = list::run(&vault.context)?;
    assert!(
        rows.is_empty(),
        "the fixture's tickets belong to another repo: nothing shows"
    );

    Ok(())
}

/// `new` writes `repo: "[[name]]"` (the olink form — asserted by new's
/// own happy-path test) while the fixture notes carry the bare form.
/// Both must match the same repo, and the listing normalizes to the
/// bare name. Hand-planting the note keeps this a pure `list` test.
#[test]
fn olink_wrapped_repo_fields_match_the_current_repo() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "NGP-500-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["list"], &repo)?;
    std::fs::write(
        vault.context.features_path().join("NGP-500.md"),
        "---\nstatus: \"In Progress\"\nrepo: \"[[connected-module-item-api]]\"\ndone: false\n---\nbody\n",
    )?;

    let rows = list::run(&vault.context)?;
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["BCP-74043", "NGP-1", "NGP-500"],
        "the olink-form repo field matches the current repo like the bare form"
    );
    assert_eq!(
        rows[2].repo.as_deref(),
        Some("connected-module-item-api"),
        "the listing shows the bare name, not the raw [[...]] field"
    );

    Ok(())
}
