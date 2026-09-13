//! Integration tests for `ocli new` (D6): fixture vault + fixture repo in
//! temp dirs (never the real vault), through the public library API only.
//! Byte-exact rendering is pinned at the unit level against the fixture
//! template with a fixed clock; these exercise the command end-to-end:
//! branch → id → render → fills → refuse-to-overwrite write.

mod common;

use common::TestVault;
use ocli::commands::new;

const ORIGIN: &str = "https://host/org/connected-module-item-api.git";

#[test]
fn creates_the_note_from_branch_and_repo() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "NGP-500-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["new"], &repo)?;

    let path = new::run(&vault.context)?;

    assert_eq!(
        path.file_name().unwrap().to_str(),
        Some("NGP-500.md"),
        "the id is the filename (D4), composed from the branch captures"
    );
    let text = std::fs::read_to_string(&path)?;
    assert!(text.starts_with("---\n"), "frontmatter first");
    assert!(text.contains("type: \"NGP\""));
    assert!(
        text.contains("- \"NGP-500\""),
        "aliases composed from captures"
    );
    assert!(
        text.contains("browse/NGP-500"),
        "jira URL placeholder resolved inside an unquoted scalar"
    );
    assert!(
        text.contains("repo: \"[[connected-module-item-api]]\""),
        "repo fill (D13): olink from the origin's name"
    );
    assert!(
        text.contains("status: \"In Progress\""),
        "every created note starts In Progress"
    );
    assert!(
        text.contains("done: false"),
        "the initial status is consistent with the template's done flag"
    );
    assert!(!text.contains("{{"), "no placeholder survives");
    assert!(text.contains("## Progress"), "D19 sections pre-seeded");
    assert!(
        text.contains("<!-- ocli:footer -->"),
        "footer marker preserved"
    );
    assert!(
        text.contains("## Descisions"),
        "the template's typo is the user's to fix, not ocli's"
    );
    Ok(())
}

#[test]
fn description_and_set_fill_the_rendered_note() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "NGP-500-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(
        &[
            "new",
            "--description",
            "Add SSO",
            "--set",
            "FeatureType=XX",
            "--repo",
            "my-repo",
        ],
        &repo,
    )?;

    let path = new::run(&vault.context)?;
    let text = std::fs::read_to_string(&path)?;

    assert!(text.contains("description: \"Add SSO\""), "D24 fill");
    assert!(
        text.contains("type: \"XX\""),
        "--set is the highest value layer (D21)"
    );
    assert!(
        text.contains("repo: \"[[my-repo]]\""),
        "--repo overrides the origin's name (D13)"
    );
    // The id is still composed from the branch, not the overridden value.
    assert_eq!(path.file_name().unwrap().to_str(), Some("NGP-500.md"));
    Ok(())
}

/// D25: the explicit key wins for the filename, and its pattern captures
/// become the Key value layer — overriding the branch's.
#[test]
fn explicit_key_overrides_the_branch_id_and_values() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "NGP-500-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["new", "BCP-900"], &repo)?;

    let path = new::run(&vault.context)?;
    let text = std::fs::read_to_string(&path)?;

    assert_eq!(path.file_name().unwrap().to_str(), Some("BCP-900.md"));
    assert!(
        text.contains("type: \"BCP\""),
        "key-derived values beat branch values"
    );
    assert!(text.contains("browse/BCP-900"));
    Ok(())
}

/// D11: `new` never rewrites an existing file — the fixture already has
/// `BCP-74043.md`, and the branch derives exactly that id.
#[test]
fn existing_note_is_refused_naming_the_path() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "BCP-74043-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["new"], &repo)?;

    let err = new::run(&vault.context).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("BCP-74043.md"),
        "the error names the existing note's path: {message}"
    );
    Ok(())
}

/// D20b: off-pattern branches are refused, naming branch and pattern.
#[test]
fn off_pattern_branch_is_refused() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "main", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["new"], &repo)?;

    let err = new::run(&vault.context).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("main"), "names the branch: {message}");
    assert!(
        message.contains("TicketNumber"),
        "names the pattern: {message}"
    );
    Ok(())
}

/// A detached HEAD has no branch name — `new` cannot infer and refuses.
#[test]
fn detached_head_is_refused() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "main", None);
    common::run_git(&repo, &["checkout", "-q", "--detach"]);
    let vault = TestVault::fixture_with_git(&["new"], &repo)?;

    let err = new::run(&vault.context).unwrap_err();
    assert!(
        err.to_string().contains("branch"),
        "says the branch is the problem: {err}"
    );
    Ok(())
}

/// Outside any repository there is no branch either — the same policy
/// error, via the degraded snapshot (D35).
#[test]
fn no_git_repository_is_refused() -> color_eyre::eyre::Result<()> {
    let vault = TestVault::fixture(&["new"])?;

    let err = new::run(&vault.context).unwrap_err();
    assert!(
        err.to_string().contains("branch"),
        "says the branch is the problem: {err}"
    );
    Ok(())
}

/// D12: an unrecognized construct in the template hard-errors naming the
/// line — never a silent pass-through into a created note.
#[test]
fn unknown_construct_in_template_is_refused() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "NGP-500-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["new"], &repo)?;
    std::fs::write(
        vault
            .context
            .config()
            .vault
            .root
            .join("templates/Feature.md"),
        "---\nstatus:\n---\nmade at {{TIME:HH:mm}}\n",
    )?;

    let err = new::run(&vault.context).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("TIME:HH:mm"),
        "names the construct: {message}"
    );
    assert!(message.contains("line 4"), "names the line: {message}");
    Ok(())
}

/// D21: a placeholder no layer provides is a loud error naming it.
#[test]
fn placeholder_without_a_value_is_refused() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "NGP-500-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["new"], &repo)?;
    std::fs::write(
        vault
            .context
            .config()
            .vault
            .root
            .join("templates/Feature.md"),
        "---\nepic: \"{{VALUE:Epic}}\"\n---\nbody\n",
    )?;

    let err = new::run(&vault.context).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("Epic"), "names the placeholder: {message}");
    Ok(())
}

/// D24 revised: a template that lacks the fill target warns and appends —
/// the template is the drift source, not a blocker.
#[test]
fn missing_fill_field_appends_with_a_warning() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "NGP-500-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["new", "--description", "Add SSO"], &repo)?;
    // A template with none of the fill targets (repo, description, status):
    // all three go through warn-and-append here.
    std::fs::write(
        vault
            .context
            .config()
            .vault
            .root
            .join("templates/Feature.md"),
        "---\nepic:\n---\nbody\n",
    )?;

    let path = new::run(&vault.context)?;
    let text = std::fs::read_to_string(&path)?;
    assert!(
        text.starts_with("---\nepic:\n"),
        "untouched fields stay first"
    );
    assert!(text.contains("---\nbody"), "delimiters stay in place");
    assert!(
        text.contains("epic:\nrepo:"),
        "appends land inside the frontmatter, before the closing delimiter"
    );
    assert!(text.contains("repo: \"[[connected-module-item-api]]\"\n"));
    assert!(text.contains("description: \"Add SSO\"\n"));
    assert!(text.contains("status: \"In Progress\"\n"));
    Ok(())
}

/// D12: a missing template file is its own error naming the path.
#[test]
fn missing_template_names_the_path() -> color_eyre::eyre::Result<()> {
    let repo_temp = tempfile::tempdir()?;
    let repo = common::fixture_repo(repo_temp.path(), "NGP-500-fix-login", Some(ORIGIN));
    let vault = TestVault::fixture_with_git(&["new"], &repo)?;
    std::fs::remove_file(
        vault
            .context
            .config()
            .vault
            .root
            .join("templates/Feature.md"),
    )?;

    let err = new::run(&vault.context).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("templates/Feature.md"),
        "names the template path: {message}"
    );
    Ok(())
}
