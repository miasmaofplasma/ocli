//! The D11 write path: edit an existing note without ever clobbering a
//! concurrent change. Optimistic concurrency — read, edit, re-read and
//! byte-compare, then swap+rename — retrying while the note keeps changing
//! under us. The field-region splices it composes live in
//! [`frontmatter`](super::frontmatter); this module owns the file level:
//! read, conflict detection, atomic replacement.

use std::{fs, io::Write, path::Path, path::PathBuf};

use crate::vault::{VaultError, features::read_file_to_str};

/// Total attempts (first try plus conflict retries) before giving up.
const MAX_ATTEMPTS: u32 = 5;

/// Edits the note at `path` through `edit`, writing the result atomically:
/// temp file in the same directory, then `rename` over the note — a reader
/// (or Obsidian) never sees a half-written file.
///
/// `edit` receives the note's *current* text so the machinery can re-apply
/// it on conflict: on a byte mismatch (someone wrote between our read and
/// our check) the loop retries with fresh content rather than clobbering.
/// A failed edit is retried only when the note actually changed — a stale
/// snapshot can make a valid edit fail (e.g. a field the concurrent write
/// just added); against unchanged content the error is real and propagates.
pub fn edit_note(
    path: &Path,
    edit: impl FnMut(&str) -> Result<String, VaultError>,
) -> Result<(), VaultError> {
    let mut edit = edit;
    let swap = swap_path(path)?;

    for _ in 0..MAX_ATTEMPTS {
        let before = read_file_to_str(path)?;
        let new = match edit(&before) {
            Ok(new) => new,
            Err(err) => {
                // Stale-edit check: retry only if the note changed under us.
                if read_file_to_str(path)? != before {
                    continue;
                }
                return Err(err);
            }
        };
        // Conflict check: the bytes we edited must still be on disk.
        if read_file_to_str(path)? != before {
            continue;
        }
        write_swap_and_rename(&swap, path, &new)?;
        return Ok(());
    }

    Err(VaultError::NoteChangedUnderUs {
        path: path.display().to_string(),
    })
}

/// Writes `contents` to the swap file, then renames it over the note.
/// A failed attempt removes the swap file — a crash leftover must never
/// surface as a phantom note.
fn write_swap_and_rename(swap: &Path, note: &Path, contents: &str) -> Result<(), VaultError> {
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(swap)?;
        file.write_all(contents.as_bytes())?;
        file.flush()?;
        fs::rename(swap, note)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(swap);
    }
    result.map_err(|error| VaultError::IoError {
        path: note.display().to_string(),
        error,
    })
}

/// The swap file's path: hidden next to the note (a dotfile — the features
/// scan skips dotfiles and Obsidian ignores them), so a crash leftover can
/// never masquerade as a note.
fn swap_path(path: &Path) -> Result<PathBuf, VaultError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| VaultError::PathNotFound {
            path: path.display().to_string(),
        })?;
    let mut swap = path.to_path_buf();
    swap.set_file_name(format!(".{file_name}.swp"));
    Ok(swap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn note(dir: &tempfile::TempDir, name: &str, contents: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn read(path: &std::path::Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn edits_apply_and_write_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = note(&dir, "BCP-1.md", "hello");
        edit_note(&path, |text| Ok(format!("{text}!"))).unwrap();
        assert_eq!(read(&path), "hello!");
    }

    /// A concurrent write landing between our read and our verify must
    /// trigger a retry against the fresh content — never a clobber.
    #[test]
    fn conflicting_write_retries_on_fresh_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = note(&dir, "BCP-2.md", "v1");
        let attempts = Cell::new(0u32);

        edit_note(&path, |text| {
            attempts.set(attempts.get() + 1);
            if attempts.get() == 1 {
                // The concurrent writer lands while we are editing.
                std::fs::write(&path, "theirs").unwrap();
            }
            Ok(format!("edited {text}"))
        })
        .unwrap();

        assert_eq!(attempts.get(), 2);
        assert_eq!(read(&path), "edited theirs");
    }

    /// An edit error against a *stale* snapshot retries (the concurrent
    /// write may have fixed it); against unchanged content it propagates.
    #[test]
    fn edit_error_retries_when_the_note_changed() {
        let dir = tempfile::tempdir().unwrap();
        let path = note(&dir, "BCP-3.md", "v1");
        let attempts = Cell::new(0u32);

        edit_note(&path, |text| {
            attempts.set(attempts.get() + 1);
            if attempts.get() == 1 {
                std::fs::write(&path, "theirs").unwrap();
                return Err(VaultError::PathNotFound {
                    path: "injected".to_string(),
                });
            }
            Ok(format!("edited {text}"))
        })
        .unwrap();

        assert_eq!(attempts.get(), 2);
        assert_eq!(read(&path), "edited theirs");
    }

    #[test]
    fn edit_error_propagates_when_the_note_did_not_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = note(&dir, "BCP-4.md", "original");

        let err = edit_note(&path, |_| {
            Err(VaultError::PathNotFound {
                path: "injected".to_string(),
            })
        })
        .unwrap_err();

        assert!(err.to_string().contains("injected"), "got: {err}");
        assert_eq!(read(&path), "original");
    }

    /// Every attempt sees the note change → bounded retries, then the
    /// dedicated error naming the path.
    #[test]
    fn exhausted_retries_error_naming_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = note(&dir, "BCP-5.md", "v1");
        let attempts = Cell::new(0u32);

        let err = edit_note(&path, |text| {
            attempts.set(attempts.get() + 1);
            // Always changes the note after our read — every verify fails.
            let _ = std::fs::write(&path, format!("noise {text}"));
            Ok("never written".to_string())
        })
        .unwrap_err();

        assert!(err.to_string().contains("BCP-5.md"), "got: {err}");
        assert_eq!(attempts.get(), MAX_ATTEMPTS);
    }

    #[test]
    fn missing_note_errors_naming_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let err = edit_note(&dir.path().join("nope.md"), |text| Ok(text.to_string())).unwrap_err();
        assert!(err.to_string().contains("nope.md"), "got: {err}");
    }

    /// The swap file must be invisible to the scan: a dotfile, and not a
    /// `.md` — a crash leftover can never become a phantom ticket.
    #[test]
    fn swap_file_is_hidden_from_the_scan() {
        let dir = tempfile::tempdir().unwrap();
        let path = note(&dir, "BCP-6.md", "x");
        let swap = swap_path(&path).unwrap();
        let name = swap.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with('.'), "hidden: {name}");
        assert_ne!(swap.extension().and_then(|e| e.to_str()), Some("md"));
    }
}
