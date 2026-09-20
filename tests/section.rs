mod common;

use std::collections::BTreeMap;

use common::TestVault;
use ocli::commands::section;
use ocli::config::{SectionFormat, SectionSpec};

const ORIGIN: &str = "https://host/org/connected-module-item-api.git";

fn spec(heading: &str, format: SectionFormat) -> SectionSpec {
    SectionSpec {
        heading: heading.to_string(),
        format,
    }
}

fn sections(pairs: &[(&str, &str, SectionFormat)]) -> BTreeMap<String, SectionSpec> {
    pairs
        .iter()
        .map(|(key, heading, format)| (key.to_string(), spec(heading, *format)))
        .collect()
}

/// `section <key> add` appends a timestamped `log` entry to an existing
/// section defined in config.
#[test]
fn appends_a_log_entry_to_a_defined_section() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "BCP-74043-fix-login", Some(ORIGIN));
    let sections = sections(&[("progress", "## Progress", SectionFormat::Log)]);
    let vault = TestVault::fixture_with_sections(
        &["section", "progress", "add", "shipped it"],
        &repo,
        sections,
    )?;

    section::run(&vault.context)?;

    let text = std::fs::read_to_string(vault.context.features_path().join("BCP-74043.md"))?;
    assert!(text.contains("## Progress"), "heading kept: {text}");
    assert!(text.contains("- kicked off"), "existing entry kept: {text}");
    assert!(text.contains("— shipped it"), "new entry appended: {text}");
    Ok(())
}

/// A user-defined `### Todo` section (any heading level, `list` format)
/// is created on demand and appended to.
#[test]
fn creates_a_user_defined_h3_list_section() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "BCP-74043-fix-login", Some(ORIGIN));
    let sections = sections(&[("todos", "### Todo", SectionFormat::List)]);
    let vault = TestVault::fixture_with_sections(
        &["section", "todos", "add", "Fix that bug"],
        &repo,
        sections,
    )?;

    section::run(&vault.context)?;

    let text = std::fs::read_to_string(vault.context.features_path().join("BCP-74043.md"))?;
    assert!(text.contains("### Todo"), "h3 heading created: {text}");
    assert!(text.contains("- [ ] Fix that bug"), "list entry: {text}");
    Ok(())
}