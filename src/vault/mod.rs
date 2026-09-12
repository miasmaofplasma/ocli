pub mod features;
pub mod frontmatter;
pub mod markdown;
pub mod note;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("could not deserialize yaml frontmatter")]
    CouldNotDeserializeFrontmatter(#[from] yaml_serde::Error),
    #[error("could not find note at {path}")]
    NoteNotFound {
        path: String,
        #[source]
        error: std::io::Error,
    },
    #[error("could not read note at {path}")]
    IoError {
        path: String,
        #[source]
        error: std::io::Error,
    },
}

/// Test helper: flattens an error's full source chain into one string.
/// The D26 diagnostics split puts causes (field names, line numbers) in
/// the source, not the top-level Display — assertions go through here.
#[cfg(test)]
pub(crate) fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    std::iter::successors(Some(error), |e| e.source())
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join(" ← ")
}
