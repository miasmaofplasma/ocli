use std::fmt::Display;

use color_eyre::eyre::eyre;
use tracing::instrument;

use crate::{
    cli,
    context::Context,
    vault::{self, note::Note},
};

pub struct Row {
    pub id: String,
    pub description: Option<String>,
    pub status: Option<String>,
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

        writeln!(f, "status: {}", self.status.as_deref().unwrap_or("-"))?;
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
            status: fm.and_then(|f| f.status.clone()),
            repo: fm.and_then(|f| f.repo.clone()),
        }
    }
}

#[instrument(skip(context))]
pub fn run(context: &Context) -> color_eyre::Result<Vec<Row>> {
    let cli::Command::List {
        all_repos: _all_repos,
        status,
    } = &context.cli().command
    else {
        return Err(eyre!("list command incorrectly called"));
    };

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

    // TODO for now returning all repos no matter the value
    // filter note by status
    let rows: Vec<Row> = notes
        .iter()
        .filter(|n| note_matches_status(n, status))
        .map(Row::new)
        .collect();

    Ok(rows)
}

fn note_matches_status(note: &Note, status: &Option<String>) -> bool {
    let Some(status) = status else {
        return true;
    };

    note.frontmatter().and_then(|f| f.status.as_deref()) == Some(status)
}
