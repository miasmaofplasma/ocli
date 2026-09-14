//! `ocli open` (D29): resolve the current ticket's note and hand an
//! `obsidian://` URI to the OS opener. Read-only: the note must already
//! exist (`new` creates it); the only side effect is the URI hand-off.

use std::path::{Path, PathBuf};

use color_eyre::eyre::{WrapErr, eyre};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use tracing::instrument;

use crate::{
    cli,
    context::Context,
    vault::{self, VaultError},
};

/// The URI handed to the OS opener: Obsidian's `open` action (D29). The
/// `path` parameter takes the note's absolute file-system path — Obsidian
/// then opens it in "the most specific vault which contains" it, so no
/// vault name is needed.
///
/// The whole path is percent-encoded (everything but unreserved
/// characters): Obsidian's URI docs require it (`/` → `%2F`, space →
/// `%20`), because an unencoded reserved character breaks the URI's
/// interpretation — worst case `#`, which would jump to a heading or
/// block instead of staying part of the path.
fn uri_for(note: &Path) -> String {
    format!(
        "obsidian://open?path={}",
        utf8_percent_encode(note.display().to_string().as_ref(), NON_ALPHANUMERIC)
    )
}

/// The note file for the current ticket: branch → id (D25), then the
/// existence check D29 requires — `open` never creates, a missing note
/// is the "run `ocli new`" moment (D26).
fn note_path(context: &Context) -> Result<PathBuf, VaultError> {
    let (branch, caps) = super::current_ticket(context)?;
    let tickets = &context.config().tickets;
    let name = vault::note_name(&branch, &tickets.pattern, &caps, &tickets.id)?;
    let path = context.features_path().join(format!("{name}.md"));

    // `is_file`, not exists: a directory at the note's path is not a
    // note. The metadata error (usually NotFound) is the real
    // `NoteNotFound` source — no synthesized failure for the common case.
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => Ok(path),
        Ok(_) => Err(VaultError::NoteNotFound {
            path: path.display().to_string(),
            error: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path exists but is not a file",
            ),
        }),
        Err(error) => Err(VaultError::NoteNotFound {
            path: path.display().to_string(),
            error,
        }),
    }
}

/// Opens the note in Obsidian. Success means the launcher accepted the
/// hand-off (nonzero launcher exits — e.g. "no handler registered" —
/// propagate as errors); it cannot mean "the note is on screen": the OS
/// gives no completion signal for URI hand-offs.
#[instrument(skip(context))]
pub fn run(context: &Context) -> color_eyre::Result<()> {
    let cli::Command::Open = &context.cli().command else {
        return Err(eyre!("open command incorrectly called"));
    };

    let note = note_path(context)?;
    let uri = uri_for(&note);
    open::that(&uri).wrap_err_with(|| format!("could not open {uri}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_percent_encodes_the_whole_path() {
        let uri = uri_for(Path::new("/tmp/My Vault/notes/features/BCP-1.md"));
        assert_eq!(
            uri, "obsidian://open?path=%2Ftmp%2FMy%20Vault%2Fnotes%2Ffeatures%2FBCP%2D1%2Emd",
            "everything but unreserved characters encoded (Obsidian docs: / → %2F, space → %20)"
        );
    }

    #[test]
    fn uri_encodes_hash_and_unicode() {
        // An unencoded `#` becomes a heading/block jump (the docs'
        // `Note%23Heading` navigation) instead of part of the path;
        // non-ASCII must travel as UTF-8 percent escapes.
        let uri = uri_for(Path::new("/vault/CAFÉ#1.md"));
        assert_eq!(uri, "obsidian://open?path=%2Fvault%2FCAF%C3%89%231%2Emd");
    }
}
