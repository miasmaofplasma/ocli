use std::fmt::Display;

use color_eyre::eyre::eyre;
use tracing::instrument;

use crate::{
    cli,
    context::Context,
    status::Status,
    vault::{self, note::Note},
};

pub struct Row {
    pub id: String,
    pub description: Option<String>,
    pub status: Option<Status>,
    pub repo: Option<String>,
}

impl Display for Row {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "id: {}", self.id)?;
        writeln!(
            f,
            "description: {}",
            self.description.as_deref().unwrap_or("-")
        )?;

        writeln!(
            f,
            "status: {}",
            self.status
                .map(|s| s.to_string())
                .unwrap_or("-".to_string())
        )?;
        writeln!(f, "repository: {}", self.repo.as_deref().unwrap_or("-"))?;
        Ok(())
    }
}

impl Row {
    fn new(note: &Note) -> Self {
        let fm = note.frontmatter();
        Self {
            id: note.name().to_string(),
            description: fm.and_then(|f| f.description.clone()),
            status: fm.and_then(|f| f.status),
            // The bare repo name — the raw field is `[[name]]` on
            // ocli-created notes; the view normalizes (same helper the
            // repo filter uses).
            repo: note_repo(note).map(str::to_string),
        }
    }
}

#[instrument(skip(context))]
pub fn run(context: &Context) -> color_eyre::Result<Vec<Row>> {
    let cli::Command::List { all_repos, status } = &context.cli().command else {
        return Err(eyre!("list command incorrectly called"));
    };
    let status = *status;

    let feature_notes_path = context.features_path();
    let note_paths = vault::features::feature_note_paths(&feature_notes_path)?;
    let notes: Vec<Note> = note_paths
        .iter()
        .filter_map(|p| match Note::load(p) {
            Ok(note) => Some(note),
            Err(e) => {
                tracing::warn!(path = %p.display(), error = %e, "skipping unreadable note");
                None
            }
        })
        .collect();

    // Repo filter (D13): the current repo's tickets only, `--all-repos`
    // widens. Outside a repository there is no identity to filter by —
    // the degraded snapshot carries `repo_name: None`, and filtering by
    // an invented name would silently hide every note, so the vault's
    // whole listing shows (same as `--all-repos`).
    let repo_name = (!all_repos)
        .then(|| context.git().repo_name.clone())
        .flatten();

    let rows: Vec<Row> = notes
        .iter()
        .filter(|n| note_matches_status(n, &status))
        .filter(|n| note_matches_repo(n, repo_name.as_deref()))
        .map(Row::new)
        .collect();

    Ok(rows)
}

fn note_matches_status(note: &Note, status: &Option<Status>) -> bool {
    let Some(status) = status else {
        return true;
    };

    note.frontmatter().and_then(|f| f.status.as_ref()) == Some(status)
}

/// The repo a note belongs to: its `repo` field with ocli's olink wrapper
/// stripped — `[[name]]` (what `new` writes) and bare `name` (what
/// hand-written notes carry) name the same repository.
fn note_repo(note: &Note) -> Option<&str> {
    note.frontmatter()
        .and_then(|f| f.repo.as_deref())
        .map(|repo| repo.trim_start_matches('[').trim_end_matches(']'))
}

fn note_matches_repo(note: &Note, repo_name: Option<&str>) -> bool {
    let Some(repo_name) = repo_name else {
        return true;
    };

    note_repo(note) == Some(repo_name)
}
