pub mod frontmatter;
pub mod markdown;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("could not deserialize yaml frontmatter {0}")]
    CouldNotDeserializeFrontmatter(#[from] yaml_serde::Error),
}
