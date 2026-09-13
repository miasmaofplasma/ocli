use std::path::Path;

use crate::vault::{
    VaultError,
    frontmatter::{Frontmatter, deserialize_frontmatter, inner_yaml},
    markdown::{self},
};

/// A loaded note: owns its text and the owned frontmatter projection;
/// the text is available for re-parsing via [`Note::text`].
#[derive(Debug)]
pub struct Note {
    /// File stem of the note file — for feature notes this is the ticket
    /// ID (`BCP-74043.md` → `"BCP-74043"`; D4).
    name: String,
    frontmatter: Option<Frontmatter>,
    text: String,
}

impl Note {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, VaultError> {
        let path = path.as_ref();
        let text = match std::fs::read_to_string(path) {
            Ok(file) => file,
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => {
                    return Err(VaultError::NoteNotFound {
                        path: path.display().to_string(),
                        error: e,
                    });
                }
                _ => {
                    return Err(VaultError::IoError {
                        path: path.display().to_string(),
                        error: e,
                    });
                }
            },
        };

        // The note name is the file's stem (D4: the filename is the ID).
        // `file_stem` is effectively always present for a readable file;
        // the fallback keeps non-stem paths representable rather than
        // inventing a new error case.
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());

        let document = markdown::parse(&text);
        let fm_text = document.frontmatter().map(|s| document.get(s));
        let frontmatter = match fm_text {
            Some(text) => {
                let fm = inner_yaml(text);
                Some(deserialize_frontmatter(fm)?)
            }
            None => None,
        };

        Ok(Note {
            name,
            frontmatter,
            text,
        })
    }

    pub fn frontmatter(&self) -> Option<&Frontmatter> {
        self.frontmatter.as_ref()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::error_chain;

    fn write_note(dir: &Path, name: &str, contents: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn lf_note_loads_name_and_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_note(
            dir.path(),
            "BCP-74043.md",
            "---\nstatus: In Progress\ndone: false\n---\n\nbody\n",
        );
        let note = Note::load(&path).unwrap();
        assert_eq!(note.name(), "BCP-74043");
        let fm = note.frontmatter().unwrap();
        assert_eq!(fm.status.as_deref(), Some("In Progress"));
        assert_eq!(fm.done, Some(false));
    }

    #[test]
    fn crlf_note_parses_frontmatter() {
        // Regression: the old byte-arithmetic shell strip mangled CRLF notes.
        let dir = tempfile::tempdir().unwrap();
        let path = write_note(
            dir.path(),
            "BCP-74044.md",
            "---\r\nstatus: In Review\r\n---\r\nbody\r\n",
        );
        let note = Note::load(&path).unwrap();
        assert_eq!(
            note.frontmatter().unwrap().status.as_deref(),
            Some("In Review")
        );
    }

    #[test]
    fn unterminated_closing_delimiter() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_note(dir.path(), "X-1.md", "---\nstatus: Complete\n---");
        let note = Note::load(&path).unwrap();
        assert_eq!(
            note.frontmatter().unwrap().status.as_deref(),
            Some("Complete")
        );
    }

    #[test]
    fn empty_frontmatter_block_is_some_all_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_note(dir.path(), "X-2.md", "---\n---\nbody\n");
        let note = Note::load(&path).unwrap();
        let fm = note.frontmatter().unwrap();
        assert_eq!(fm.status, None);
        assert_eq!(fm.done, None);
    }

    #[test]
    fn no_frontmatter_block_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_note(dir.path(), "X-3.md", "just prose\n");
        let note = Note::load(&path).unwrap();
        assert!(note.frontmatter().is_none());
    }

    #[test]
    fn missing_note_names_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("BCP-99999.md");
        let err = Note::load(&path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("could not find note at"), "got: {msg}");
        assert!(msg.contains("BCP-99999.md"), "got: {msg}");
    }

    #[test]
    fn wrong_typed_field_errors_naming_the_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_note(dir.path(), "X-4.md", "---\nstatus: [a, b]\n---\n");
        let err = Note::load(&path).unwrap_err();
        assert!(error_chain(&err).contains("status"), "got: {err}");
    }
}
