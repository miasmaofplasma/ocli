//! The per-invocation environment (D27 boundary): resolved config plus the
//! CLI invocation, validated against the filesystem once, at construction.
//! After `new`, a `Context` is inert data — commands take `&Context` and
//! never touch discovery or validation.

use std::path::PathBuf;

use thiserror::Error;

use crate::{cli::Cli, config::Config};

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("vault directory does not exist: {0}")]
    VaultDoesNotExist(PathBuf),
}

#[derive(Debug)]
pub struct Context {
    config: Config,
    cli: Cli,
}

impl Context {
    /// Validates runtime facts the config layer deliberately doesn't know
    /// about (the vault directory can appear or vanish between config load
    /// and command execution — checking it here checks *now*).
    pub fn new(config: Config, cli: Cli) -> Result<Self, ContextError> {
        // is_dir, not exists: a file at the configured root would pass an
        // existence check and then blow up on the first directory read.
        if !config.vault.root.is_dir() {
            return Err(ContextError::VaultDoesNotExist(
                config.vault.root.clone(),
            ));
        }

        Ok(Self { config, cli })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn cli(&self) -> &Cli {
        &self.cli
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use clap::Parser;

    use regex::Regex;

    use super::*;
    use crate::config::{
        FieldType, FrontmatterCfg, ResolvedVault, SectionsCfg, TemplateCfg, Tickets,
    };

    /// A minimal valid `Config` pointing at `root` — built directly, since
    /// config validation is deliberately filesystem-free.
    fn config_with_root(root: PathBuf) -> Config {
        Config {
            vault: ResolvedVault {
                root,
                features_dir: PathBuf::from("notes/features"),
                people_dir: PathBuf::from("notes/people"),
            },
            frontmatter: FrontmatterCfg {
                ignore: Vec::new(),
                types: BTreeMap::new(),
            },
            template: TemplateCfg::default(),
            tickets: Tickets {
                pattern: Regex::new(r"^(?<Key>[A-Z]+)-(?<Num>\d+)").unwrap(),
                id: "{Key}-{Num}".to_string(),
            },
            sections: SectionsCfg::default(),
        }
    }

    fn cli() -> Cli {
        Cli::parse_from(["ocli", "list"])
    }

    #[test]
    fn accepts_existing_vault_directory() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(config_with_root(dir.path().to_path_buf()), cli());
        assert!(ctx.is_ok());
    }

    #[test]
    fn rejects_missing_vault_directory() {
        let config = config_with_root(PathBuf::from("/definitely/not/a/vault"));
        let err = Context::new(config, cli()).unwrap_err();
        assert!(matches!(err, ContextError::VaultDoesNotExist(_)));
        assert!(
            err.to_string().contains("/definitely/not/a/vault"),
            "error should name the path: {err}"
        );
    }

    #[test]
    fn rejects_vault_root_that_is_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not-a-dir");
        std::fs::write(&file, "x").unwrap();
        let err = Context::new(config_with_root(file), cli()).unwrap_err();
        assert!(matches!(err, ContextError::VaultDoesNotExist(_)));
    }
}
