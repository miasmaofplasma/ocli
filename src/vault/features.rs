use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::vault::VaultError;

#[instrument]
pub fn feature_note_paths(feature_dir: &Path) -> Result<Vec<PathBuf>, VaultError> {
    let entries = std::fs::read_dir(feature_dir).map_err(|e| VaultError::IoError {
        path: feature_dir.display().to_string(),
        error: e,
    })?;

    let mut notes:Vec<PathBuf> = entries
        .filter_map(|res| {
            res.map(|entry| entry.path())
                .inspect_err(|e| tracing::warn!(path = %feature_dir.display(), error = %e, "skipping unreadable directory entry"))
                .ok()
                .filter(|entry| is_note_file(entry))
        })
        .collect();
    notes.sort();

    Ok(notes)
}

pub fn read_file_to_str(path: &Path) -> Result<String, VaultError> {
    match std::fs::read_to_string(path) {
        Ok(file) => Ok(file),
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => Err(VaultError::NoteNotFound {
                path: path.display().to_string(),
                error: e,
            }),
            _ => Err(VaultError::IoError {
                path: path.display().to_string(),
                error: e,
            }),
        },
    }
}

fn is_note_file(path: &Path) -> bool {
    path.is_file()
        && path.extension().is_some_and(|ext| ext == "md")
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_none_or(|n| !n.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates `rel` under `dir`, with parent directories, containing "x".
    fn create(dir: &Path, rel: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "x").unwrap();
    }

    #[test]
    fn lists_only_visible_markdown_files_sorted() {
        let dir = tempfile::tempdir().unwrap();
        create(dir.path(), "BCP-74043.md");
        create(dir.path(), "NGP-1.md");
        // Excluded: not markdown, hidden, and a directory (its .md child
        // must not cause recursion or inclusion).
        create(dir.path(), "notes.txt");
        create(dir.path(), ".hidden.md");
        create(dir.path(), "sub/BCP-99999.md");

        let paths = feature_note_paths(dir.path()).unwrap();
        assert_eq!(
            paths,
            vec![dir.path().join("BCP-74043.md"), dir.path().join("NGP-1.md")],
            "only visible .md files, sorted by name"
        );
    }

    #[test]
    fn empty_dir_is_ok_not_an_error() {
        // A vault with no tickets yet is a valid state for `list`, not a
        // signal — the missing *directory* is the signal (Context guards it).
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            feature_note_paths(dir.path()).unwrap(),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn missing_dir_errors_naming_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such-dir");
        let err = feature_note_paths(&missing).unwrap_err();
        assert!(err.to_string().contains("no-such-dir"), "got: {err}");
    }
}
