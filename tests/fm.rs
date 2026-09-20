mod common;

use common::TestVault;
use ocli::commands::fm;

const ORIGIN: &str = "https://host/org/connected-module-item-api.git";

/// The D11 contract: the edit replaces one field's region and leaves every
/// other byte — sibling fields, the body, ocli's `##` sections.
#[test]
fn sets_a_field_and_preserves_the_note() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "BCP-74043-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["fm", "description", "Updated"], &repo)?;

    fm::run(&vault.context)?;

    let text = std::fs::read_to_string(vault.context.features_path().join("BCP-74043.md"))?;
    assert!(
        text.contains("description: \"Updated\""),
        "field set with D18 emission: {text}"
    );
    assert!(text.contains("status: In Progress"), "sibling kept: {text}");
    assert!(text.contains("repo: connected-module-item-api"), "{text}");
    assert!(text.contains("done: false"), "{text}");
    assert!(text.contains("tags:\n  - feature"), "block list kept: {text}");
    assert!(text.contains("# Description"), "body kept: {text}");
    assert!(text.contains("- kicked off"), "section kept: {text}");
    Ok(())
}

/// D16 (revised): `fm` writes managed fields raw — no `done:` sync here,
/// that lives in `ocli status`.
#[test]
fn writes_managed_fields_without_done_sync() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "BCP-74043-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["fm", "status", "Complete"], &repo)?;

    fm::run(&vault.context)?;

    let text = std::fs::read_to_string(vault.context.features_path().join("BCP-74043.md"))?;
    assert!(text.contains("status: \"Complete\""), "got: {text}");
    assert!(text.contains("done: false"), "fm does not sync done: {text}");
    Ok(())
}

/// D18: ignore-listed fields belong to other tooling — never written.
#[test]
fn refuses_ignored_fields() -> color_eyre::eyre::Result<()> {
    let vault = TestVault::fixture(&["fm", "relates-to", "x"])?;

    let err = fm::run(&vault.context).unwrap_err();

    assert!(err.to_string().contains("ignore list"), "got: {err}");
    Ok(())
}

/// `fm` is strict (D16): a misspelled field errors with the closest-key
/// suggestion instead of creating a junk field.
#[test]
fn misspelled_field_suggests_the_closest_key() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "BCP-74043-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["fm", "statuz", "Backlog"], &repo)?;

    let err = fm::run(&vault.context).unwrap_err();

    let message = err.to_string();
    assert!(message.contains("statuz"), "got: {message}");
    assert!(message.contains("did you mean"), "got: {message}");
    Ok(())
}
