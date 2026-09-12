//! Integration tests for `ocli list` (D6): every test runs against a copy
//! of the checked-in fixture vault, through the public library API only.

mod common;

use common::TestVault;
use ocli::commands::list;

#[test]
fn lists_fixture_notes_skipping_malformed() -> color_eyre::eyre::Result<()> {
    let vault = TestVault::fixture(&["list"])?;
    let rows = list::run(&vault.context)?;

    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["BCP-74043", "NGP-1", "no-fm"],
        "scan order preserved; broken.md skipped without failing the listing"
    );

    let bcp = &rows[0];
    assert_eq!(
        bcp.description.as_deref(),
        Some("Add SSO to the settings page")
    );
    assert_eq!(bcp.status.as_deref(), Some("In Progress"));
    assert_eq!(bcp.repo.as_deref(), Some("connected-module-item-api"));

    let ngp = &rows[1];
    assert_eq!(
        ngp.description, None,
        "no title fallback (D24): an empty description stays empty"
    );
    assert_eq!(ngp.status.as_deref(), Some("Complete"));

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
fn status_filter_is_case_sensitive() -> color_eyre::eyre::Result<()> {
    let vault = TestVault::fixture(&["list", "--status", "in progress"])?;
    let rows = list::run(&vault.context)?;

    assert!(
        rows.is_empty(),
        "exact match, no case folding: a near-miss status yields nothing"
    );

    Ok(())
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
