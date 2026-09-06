use color_eyre::eyre::WrapErr;
use regex::Regex;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::Deserialize;
use thiserror::Error;

/// Default `[vault]` sub-paths, relative to the vault root (D23).
pub const DEFAULT_FEATURES_DIR: &str = "notes/features";
pub const DEFAULT_PEOPLE_DIR: &str = "notes/people";

/// Frontmatter fields owned by Obsidian/Meta Bind; ocli refuses to edit
/// these unless overridden in config (D16/D18).
pub const DEFAULT_IGNORE: &[&str] = &["relates-to", "blocked-by"];

/// Default `[tickets]` values (D20/D25).
pub const DEFAULT_BRANCH_PATTERN: &str = r"^(?<FeatureType>[A-Z]+)-(?<TicketNumber>\d+)";
pub const DEFAULT_ID_TEMPLATE: &str = "{FeatureType}-{TicketNumber}";

/// Default CLI-owned `##` section names (D19).
pub const DEFAULT_SECTION_PROGRESS: &str = "Progress";
pub const DEFAULT_SECTION_NOTES: &str = "Notes";
pub const DEFAULT_SECTION_DECISIONS: &str = "Decisions";
pub const DEFAULT_SECTION_QUESTIONS: &str = "Open Questions";
/// Default QuickAdd template path, relative to the vault root (D12/D22).
pub const DEFAULT_TEMPLATE_PATH: &str = "templates/Feature.md";

/// Frontmatter fields with code-level owners (D16); `fm` and config
/// `[frontmatter]` types refuse them. (field, owning command).
pub const MANAGED_FIELDS: &[(&str, &str)] = &[
    ("status", "ocli status"),
    ("done", "ocli status"),
    ("Created", "ocli new"),
    ("repo", "ocli new"),
    ("owner", "ocli new"),
];

#[derive(Debug, Error)]
pub enum ConfigFileError {
    #[error("{0} is not a valid field type")]
    FieldTypeError(String),
    /// Recoverable: the caller may proceed when the vault root comes from
    /// --vault or OCLI_VAULT instead of the config file.
    #[error("could not find the config file at {path}")]
    ConfigFileNotFound {
        path: String,
        #[source]
        error: std::io::Error,
    },
    /// Terminal: the config file exists but can't be read.
    #[error("could not read the config file at {path}")]
    Io {
        path: String,
        #[source]
        error: std::io::Error,
    },
    /// Terminal: the file exists but isn't valid TOML / doesn't match the schema.
    #[error("invalid config in {path}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(
        "frontmatter field {field:?} is managed by ocli; use {owner} (or remove the type from config)"
    )]
    ManagedField { field: String, owner: &'static str },
    #[error(
        "frontmatter field {field:?} is on the ignore list but also has a configured type; remove one of the two"
    )]
    IgnoreTypeCollision { field: String },
    #[error("tickets.branch_pattern {pattern:?} is not a valid regex")]
    Regex {
        pattern: String,
        #[source]
        source: regex::Error,
    },
    #[error(
        "tickets.branch_pattern has unnamed capture groups; every group must be named, e.g. (?<FeatureType>...)"
    )]
    UnnamedCaptures,
    #[error(
        "tickets.branch_pattern has no named capture groups; the id template needs at least one, e.g. (?<FeatureType>...)"
    )]
    NoNamedCaptures,
    #[error(
        "tickets.id {template:?} has unbalanced or empty braces; placeholders look like {{FeatureType}}"
    )]
    MalformedIdTemplate { template: String },
    #[error(
        "tickets.id placeholder {name:?} is not a named capture group in tickets.branch_pattern"
    )]
    UnknownPlaceholder { name: String },
    #[error("vault.root must be an absolute path, got {0:?}")]
    RelativeRoot(PathBuf),
    #[error("no vault root: set [vault] root in the config file, set OCLI_VAULT, or pass --vault")]
    MissingVaultRoot,
}

#[derive(Debug, Deserialize)]
pub struct VaultDirs {
    /// Resolved from --vault > OCLI_VAULT > this file (D30); `None` here
    /// means "not yet resolved" — [`ConfigFile::validate`] enforces presence.
    pub root: Option<PathBuf>,
    #[serde(default = "default_features_dir")]
    pub features_dir: PathBuf,
    #[serde(default = "default_people_dir")]
    pub people_dir: PathBuf,
}

impl Default for VaultDirs {
    fn default() -> Self {
        Self {
            root: None,
            features_dir: default_features_dir(),
            people_dir: default_people_dir(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum FieldType {
    String,
    Int,
    Float,
    Bool,
    Olink,
    List(Box<FieldType>),
}

impl<'de> Deserialize<'de> for FieldType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl FromStr for FieldType {
    type Err = ConfigFileError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "string" => Ok(Self::String),
            "int" => Ok(Self::Int),
            "float" => Ok(Self::Float),
            "bool" => Ok(Self::Bool),
            "olink" => Ok(Self::Olink),
            other => {
                let inner = other
                    .strip_prefix("list<")
                    .and_then(|r| r.strip_suffix(">"))
                    .ok_or_else(|| ConfigFileError::FieldTypeError(other.into()))?;
                Ok(Self::List(Box::new(inner.parse()?)))
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FrontmatterCfg {
    #[serde(default = "default_ignore")]
    pub ignore: Vec<String>,
    #[serde(flatten)]
    pub types: BTreeMap<String, FieldType>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct TicketCfg {
    branch_pattern: String,
    id: String,
}

impl Default for TicketCfg {
    fn default() -> Self {
        Self {
            branch_pattern: DEFAULT_BRANCH_PATTERN.into(),
            id: DEFAULT_ID_TEMPLATE.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct SectionsCfg {
    pub progress: String,
    pub notes: String,
    pub decisions: String,
    pub questions: String,
}

impl Default for FrontmatterCfg {
    fn default() -> Self {
        Self {
            ignore: default_ignore(),
            types: BTreeMap::new(),
        }
    }
}

impl Default for SectionsCfg {
    fn default() -> Self {
        Self {
            progress: DEFAULT_SECTION_PROGRESS.into(),
            notes: DEFAULT_SECTION_NOTES.into(),
            decisions: DEFAULT_SECTION_DECISIONS.into(),
            questions: DEFAULT_SECTION_QUESTIONS.into(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    vault: VaultDirs,
    #[serde(default)]
    frontmatter: FrontmatterCfg,
    #[serde(default)]
    template: TemplateCfg,
    #[serde(default)]
    tickets: TicketCfg,
    #[serde(default)]
    sections: SectionsCfg,
}

fn default_features_dir() -> PathBuf {
    PathBuf::from(DEFAULT_FEATURES_DIR)
}

fn default_people_dir() -> PathBuf {
    PathBuf::from(DEFAULT_PEOPLE_DIR)
}

/// `[template]` table (D12/D22): QuickAdd template path relative to the
/// vault root, plus static `{{VALUE:Name}}` defaults. Values are literal
/// text — quoting is the template author's business (D21/D22).
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct TemplateCfg {
    pub path: PathBuf,
    pub values: BTreeMap<String, String>,
}

impl Default for TemplateCfg {
    fn default() -> Self {
        Self {
            path: PathBuf::from(DEFAULT_TEMPLATE_PATH),
            values: BTreeMap::new(),
        }
    }
}

fn default_ignore() -> Vec<String> {
    DEFAULT_IGNORE.iter().map(|&s| s.to_string()).collect()
}

/// Resolved vault paths (D30 boundary): the root is always present —
/// `validate` refuses to build this type otherwise.
#[derive(Debug)]
pub struct ResolvedVault {
    pub root: PathBuf,
    pub features_dir: PathBuf,
    pub people_dir: PathBuf,
}

/// Resolved, validated configuration (D27 boundary). Built only by
/// [`ConfigFile::validate`]; the rest of the program sees nothing else.
#[derive(Debug)]
pub struct Config {
    pub vault: ResolvedVault,
    pub frontmatter: FrontmatterCfg,
    pub template: TemplateCfg,
    pub tickets: Tickets,
    pub sections: SectionsCfg,
}

/// Compiled ticket config (D20/D25): regex compiled once, id template
/// verified against the pattern's named captures.
#[derive(Debug)]
pub struct Tickets {
    pub pattern: Regex,
    pub id: String,
}
impl Tickets {
    /// Ticket ID for a branch match: named captures → id template (D21).
    /// Missing captures are a caller error; the caller checks `is_match`.
    pub fn id_for(&self, caps: &regex::Captures<'_>) -> String {
        let mut out = self.id.clone();
        for name in self.pattern.capture_names().flatten() {
            if let Some(value) = caps.name(name) {
                out = out.replace(&format!("{{{name}}}"), value.as_str());
            }
        }
        out
    }
}

impl ConfigFile {
    /// Read and parse `path` — the grammar layer of the D30 split. Failures
    /// here are [`ConfigFileError`]s the caller may recover from (e.g. the
    /// vault root coming from --vault or OCLI_VAULT). Policy checks live in
    /// [`ConfigFile::validate`], failing as [`ConfigError`].
    fn load(path: &Path) -> Result<Self, ConfigFileError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(ConfigFileError::ConfigFileNotFound {
                    path: path.display().to_string(),
                    error,
                });
            }
            Err(error) => {
                return Err(ConfigFileError::Io {
                    path: path.display().to_string(),
                    error,
                });
            }
        };
        toml::from_str(&text).map_err(|source| ConfigFileError::Parse {
            path: path.display().to_string(),
            source,
        })
    }

    /// Semantic validation (D16/D18/D20/D25). Consumes the raw file and
    /// produces the compiled [`Config`]; unvalidated data does not escape.
    fn validate(self) -> Result<Config, ConfigError> {
        let root = self
            .vault
            .root
            .clone()
            .ok_or(ConfigError::MissingVaultRoot)?;
        if !root.is_absolute() {
            return Err(ConfigError::RelativeRoot(root));
        }

        for name in self.frontmatter.types.keys() {
            if let Some((_, owner)) = MANAGED_FIELDS.iter().find(|(f, _)| f == name) {
                return Err(ConfigError::ManagedField {
                    field: name.clone(),
                    owner,
                });
            }
            if self.frontmatter.ignore.iter().any(|ig| ig == name) {
                return Err(ConfigError::IgnoreTypeCollision {
                    field: name.clone(),
                });
            }
        }

        let pattern =
            Regex::new(&self.tickets.branch_pattern).map_err(|source| ConfigError::Regex {
                pattern: self.tickets.branch_pattern.clone(),
                source,
            })?;

        let names: Vec<Option<&str>> = pattern.capture_names().collect();
        if names.len() > 1 && names[1..].iter().any(Option::is_none) {
            return Err(ConfigError::UnnamedCaptures);
        }
        let named: Vec<&str> = names.into_iter().flatten().collect();
        if named.len() <= 1 {
            // Only the implicit whole-match group — no named captures.
            return Err(ConfigError::NoNamedCaptures);
        }

        for name in extract_placeholders(&self.tickets.id).map_err(|_| {
            ConfigError::MalformedIdTemplate {
                template: self.tickets.id.clone(),
            }
        })? {
            if !named.contains(&name) {
                return Err(ConfigError::UnknownPlaceholder {
                    name: name.to_string(),
                });
            }
        }

        Ok(Config {
            vault: ResolvedVault {
                root,
                features_dir: self.vault.features_dir,
                people_dir: self.vault.people_dir,
            },
            frontmatter: self.frontmatter,
            template: self.template,
            tickets: Tickets {
                pattern,
                id: self.tickets.id,
            },
            sections: self.sections,
        })
    }
}

impl Config {
    /// Single entry point: resolves the config path (`OCLI_CONFIG` or the
    /// default location, D7), reads + parses it, resolves the vault root
    /// (`vault_override` > `OCLI_VAULT` > config file, D30), validates.
    ///
    /// A missing config file is recoverable when a vault root comes from
    /// elsewhere (defaults carry the rest); it is an onboarding error
    /// otherwise (D26: a missing config is a signal, never auto-created).
    pub fn load(vault_override: Option<PathBuf>) -> color_eyre::eyre::Result<Config> {
        let config_env = std::env::var_os("OCLI_CONFIG").map(PathBuf::from);
        let vault_env = std::env::var_os("OCLI_VAULT").map(PathBuf::from);
        let (source, path) = match config_env {
            Some(p) => (ConfigSource::EnvOverride, p),
            None => (ConfigSource::Default, default_config_path()?),
        };
        Self::load_from(source, &path, vault_override, vault_env)
    }

    /// Env-free core of [`Config::load`]; `main` supplies the environment,
    /// tests supply explicit values.
    fn load_from(
        source: ConfigSource,
        path: &Path,
        vault_override: Option<PathBuf>,
        vault_env: Option<PathBuf>,
    ) -> color_eyre::eyre::Result<Config> {
        tracing::debug!(path = %path.display(), ?source, "resolving config");

        let vault_available = vault_override.is_some() || vault_env.is_some();
        let mut file = match ConfigFile::load(path) {
            Ok(file) => file,
            Err(ConfigFileError::ConfigFileNotFound { path, .. }) => {
                match (source, vault_available) {
                    // An explicit OCLI_CONFIG that misses is a typo signal —
                    // recoverable vault sources must not silently mask it.
                    (ConfigSource::EnvOverride, _) => {
                        return Err(color_eyre::eyre::eyre!(
                            "OCLI_CONFIG points to {path}, which does not exist"
                        ));
                    }
                    // Fresh install with a vault from elsewhere: defaults suffice.
                    (ConfigSource::Default, true) => {
                        tracing::debug!("no config file; continuing on defaults");
                        ConfigFile::default()
                    }
                    // Fresh install with no vault anywhere: onboarding error.
                    (ConfigSource::Default, false) => {
                        return Err(color_eyre::eyre::eyre!(
                            "no config file at {path}\n\n\
                             Create {path} containing:\n\n\
                             [vault]\n\
                             root = \"/path/to/your/vault\"\n\n\
                             or pass a vault root via OCLI_VAULT or --vault"
                        ));
                    }
                }
            }
            Err(e) => return Err(e.into()),
        };

        // Precedence (D30): --vault > OCLI_VAULT > config file.
        file.vault.root = vault_override.or(vault_env).or(file.vault.root);
        file.validate()
            .wrap_err_with(|| format!("invalid config in {}", path.display()))
    }
}

/// Where a config path came from — drives missing-file diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// The standard per-user location (D7).
    Default,
    /// The `OCLI_CONFIG` environment variable.
    EnvOverride,
}

/// The standard config location (D7): `~/.config/ocli/config.toml` on
/// Linux, platform-equivalent elsewhere via `directories`.
pub fn default_config_path() -> color_eyre::eyre::Result<PathBuf> {
    directories::ProjectDirs::from("", "", "ocli")
        .map(|dirs| dirs.config_dir().join("config.toml"))
        .ok_or_else(|| {
            color_eyre::eyre::eyre!("cannot determine config directory: no home directory")
        })
}

/// Extracts `{Name}` placeholder names from an id template. `Err(())` on
/// unbalanced braces or an empty name.
fn extract_placeholders(template: &str) -> Result<Vec<&str>, ()> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let close = rest[start + 1..].find('}').ok_or(())?;
        let name = &rest[start + 1..start + 1 + close];
        if name.is_empty() {
            return Err(());
        }
        out.push(name);
        rest = &rest[start + 1 + close + 1..];
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full config: every table present, every field explicit.
    const FULL: &str = r#"
        [vault]
        root = "/home/me/vault"
        features_dir = "my/features"
        people_dir = "my/people"

        [frontmatter]
        ignore = ["custom-kept"]
        estimate = "int"
        tags = "list<olink>"

        [tickets]
        branch_pattern = '^(?<Key>[A-Z]+)-(?<Num>\d+)-'
        id = '{Key}-{Num}'

        [sections]
        progress = "Log"
        notes = "Scratch"
        decisions = "Calls"
        questions = "Qs"
    "#;

    /// Minimal config: only the required key.
    const MINIMAL: &str = r#"
        [vault]
        root = "/home/me/vault"
    "#;

    fn parse(s: &str) -> ConfigFile {
        toml::from_str(s).unwrap_or_else(|e| panic!("parse failed: {e}\n---\n{s}"))
    }

    #[test]
    fn full_config_parses_all_fields() {
        let cfg = parse(FULL);

        assert_eq!(cfg.vault.root, Some(PathBuf::from("/home/me/vault")));
        assert_eq!(cfg.vault.features_dir, PathBuf::from("my/features"));
        assert_eq!(cfg.vault.people_dir, PathBuf::from("my/people"));

        assert_eq!(cfg.frontmatter.ignore, vec!["custom-kept"]);
        assert_eq!(cfg.frontmatter.types.get("estimate"), Some(&FieldType::Int));
        assert_eq!(
            cfg.frontmatter.types.get("tags"),
            Some(&FieldType::List(Box::new(FieldType::Olink)))
        );

        assert_eq!(cfg.tickets.branch_pattern, r"^(?<Key>[A-Z]+)-(?<Num>\d+)-");
        assert_eq!(cfg.tickets.id, "{Key}-{Num}");

        assert_eq!(cfg.sections.progress, "Log");
        assert_eq!(cfg.sections.notes, "Scratch");
        assert_eq!(cfg.sections.decisions, "Calls");
        assert_eq!(cfg.sections.questions, "Qs");
    }

    #[test]
    fn minimal_config_applies_all_defaults() {
        let cfg = parse(MINIMAL);

        // Explicit.
        assert_eq!(cfg.vault.root, Some(PathBuf::from("/home/me/vault")));

        // Defaults asserted against the consts (not re-typed literals), so a
        // const change that forgets the serde wiring breaks this test.
        assert_eq!(cfg.vault.features_dir, PathBuf::from(DEFAULT_FEATURES_DIR));
        assert_eq!(cfg.vault.people_dir, PathBuf::from(DEFAULT_PEOPLE_DIR));

        assert_eq!(cfg.frontmatter.ignore, default_ignore());
        assert!(cfg.frontmatter.types.is_empty());

        assert_eq!(cfg.tickets.branch_pattern, DEFAULT_BRANCH_PATTERN);
        assert_eq!(cfg.tickets.id, DEFAULT_ID_TEMPLATE);

        assert_eq!(cfg.sections.progress, DEFAULT_SECTION_PROGRESS);
        assert_eq!(cfg.sections.notes, DEFAULT_SECTION_NOTES);
        assert_eq!(cfg.sections.decisions, DEFAULT_SECTION_DECISIONS);
        assert_eq!(cfg.sections.questions, DEFAULT_SECTION_QUESTIONS);
    }

    #[test]
    fn partial_overrides_keep_siblings() {
        // [sections] with one key: that key wins, the rest default.
        let cfg = parse(
            r#"
            [vault]
            root = "/v"

            [frontmatter]

            [sections]
            questions = "Open Items"
        "#,
        );
        assert_eq!(cfg.sections.questions, "Open Items");
        assert_eq!(cfg.sections.progress, DEFAULT_SECTION_PROGRESS);

        // [tickets] with only a pattern: id defaults.
        let cfg2: ConfigFile = toml::from_str(
            r#"
            [vault]
            root = "/v"

            [frontmatter]

            [tickets]
            branch_pattern = 'x'
        "#,
        )
        .unwrap();
        assert_eq!(cfg2.tickets.branch_pattern, "x");
        assert_eq!(cfg2.tickets.id, DEFAULT_ID_TEMPLATE);
    }

    #[test]
    fn frontmatter_named_fields_stay_out_of_types_map() {
        let cfg = parse(
            r#"
            [vault]
            root = "/v"

            [frontmatter]
            ignore = ["relates-to"]
            estimate = "int"
        "#,
        );
        assert_eq!(cfg.frontmatter.ignore, vec!["relates-to"]);
        assert_eq!(
            cfg.frontmatter.types.get("ignore"),
            None,
            "ignore is a named field and must not leak into the types map"
        );
        assert_eq!(cfg.frontmatter.types.get("estimate"), Some(&FieldType::Int));
    }

    #[test]
    fn empty_frontmatter_table_uses_ignore_default() {
        let cfg = parse(
            r#"
            [vault]
            root = "/v"

            [frontmatter]
        "#,
        );
        assert_eq!(cfg.frontmatter.ignore, default_ignore());
    }

    #[test]
    fn missing_root_fails_validation_naming_sources() {
        // D30: absent root parses fine (grammar); validate enforces presence
        // and the error names all three resolution sources.
        let cfg: ConfigFile = toml::from_str(
            r#"
            [vault]
            features_dir = "f"

            [frontmatter]
        "#,
        )
        .unwrap();
        let err = cfg.validate().unwrap_err();
        assert!(
            matches!(err, ConfigError::MissingVaultRoot),
            "expected MissingVaultRoot: {err}"
        );
        assert!(
            err.to_string().contains("OCLI_VAULT") && err.to_string().contains("--vault"),
            "error should name the alternative sources: {err}"
        );
    }

    #[test]
    fn unknown_field_type_is_an_error() {
        let err = toml::from_str::<ConfigFile>(
            r#"
            [vault]
            root = "/v"

            [frontmatter]
            estimate = "strng"
        "#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("strng"),
            "error should quote the offending type: {err}"
        );
    }

    #[test]
    fn nested_list_type_parses() {
        let cfg: FrontmatterCfg = toml::from_str("deep = 'list<list<int>>'").unwrap();
        assert_eq!(
            cfg.types.get("deep"),
            Some(&FieldType::List(Box::new(FieldType::List(Box::new(
                FieldType::Int
            )))))
        );
    }
    // ---- validate() ------------------------------------------------------

    fn valid_minimal() -> ConfigFile {
        parse(
            r#"
            [vault]
            root = "/home/me/vault"
        "#,
        )
    }

    #[test]
    fn validate_produces_compiled_pattern_and_working_id() {
        let cfg = valid_minimal().validate().unwrap();

        let caps = cfg.tickets.pattern.captures("BCP-74043-fix-login").unwrap();
        assert_eq!(caps.name("FeatureType").unwrap().as_str(), "BCP");
        assert_eq!(caps.name("TicketNumber").unwrap().as_str(), "74043");
        assert_eq!(cfg.tickets.id_for(&caps), "BCP-74043");

        // No match on a non-ticket branch.
        assert!(!cfg.tickets.pattern.is_match("main"));
    }

    #[test]
    fn validate_rejects_unnamed_capture_groups() {
        let mut cfg = valid_minimal();
        cfg.tickets.branch_pattern = r"^(?<Key>[A-Z]+)-(\d+)".into();
        let err = cfg.validate().unwrap_err();
        assert!(
            err.to_string().contains("unnamed"),
            "error should explain the naming contract: {err}"
        );
    }

    #[test]
    fn validate_rejects_pattern_without_named_captures() {
        let mut cfg = valid_minimal();
        cfg.tickets.branch_pattern = r"[A-Z]+-\d+".into();
        assert!(matches!(
            cfg.validate().unwrap_err(),
            ConfigError::NoNamedCaptures
        ));
    }

    #[test]
    fn validate_rejects_invalid_regex_with_source() {
        let mut cfg = valid_minimal();
        cfg.tickets.branch_pattern = "(?P<".into();
        let err = cfg.validate().unwrap_err();
        assert!(matches!(err, ConfigError::Regex { .. }));
        // The invalid pattern text is quoted (D26).
        assert!(err.to_string().contains("(?P<"));
    }

    #[test]
    fn validate_rejects_managed_fields_naming_the_owner() {
        let mut cfg = valid_minimal();
        cfg.frontmatter
            .types
            .insert("status".into(), FieldType::String);
        let err = cfg.validate().unwrap_err();
        assert!(matches!(err, ConfigError::ManagedField { .. }));
        assert!(
            err.to_string().contains("ocli status"),
            "error should name the owning command: {err}"
        );
    }

    #[test]
    fn validate_rejects_ignore_list_type_collision() {
        let mut cfg = valid_minimal();
        cfg.frontmatter
            .types
            .insert("relates-to".into(), FieldType::String);
        assert!(matches!(
            cfg.validate().unwrap_err(),
            ConfigError::IgnoreTypeCollision { .. }
        ));
    }

    #[test]
    fn validate_rejects_unknown_id_placeholder() {
        let mut cfg = valid_minimal();
        cfg.tickets.id = "{NotACapture}".into();
        let err = cfg.validate().unwrap_err();
        assert!(
            matches!(err, ConfigError::UnknownPlaceholder { ref name } if name == "NotACapture"),
            "error should name the placeholder: {err}"
        );
    }

    #[test]
    fn validate_rejects_malformed_id_template() {
        let mut cfg = valid_minimal();
        cfg.tickets.id = "{FeatureType".into();
        assert!(matches!(
            cfg.validate().unwrap_err(),
            ConfigError::MalformedIdTemplate { .. }
        ));
    }

    #[test]
    fn validate_rejects_relative_root() {
        let mut cfg = valid_minimal();
        cfg.vault.root = Some("vault".into());
        assert!(matches!(
            cfg.validate().unwrap_err(),
            ConfigError::RelativeRoot(_)
        ));
    }

    #[test]
    fn load_wraps_errors_with_file_context() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[frontmatter]\nestimate = \"strng\"\n").unwrap();
        let err = Config::load_from(ConfigSource::Default, &path, None, None).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("invalid config in") && msg.contains("strng"),
            "context chain should name file and cause: {msg}"
        );
    }

    #[test]
    fn load_happy_path_returns_validated_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            format!("[vault]\nroot = \"{}\"\n", dir.path().display()),
        )
        .unwrap();
        let cfg = Config::load_from(ConfigSource::Default, &path, None, None).unwrap();
        assert_eq!(cfg.vault.root, dir.path());
        assert_eq!(cfg.sections.progress, DEFAULT_SECTION_PROGRESS);
        assert_eq!(cfg.template.path, PathBuf::from(DEFAULT_TEMPLATE_PATH));
        assert!(cfg.template.values.is_empty());
    }

    #[test]
    fn missing_config_file_recovers_when_vault_comes_from_elsewhere() {
        // D30: fresh install + --vault/OCLI_VAULT -> defaults, no file needed.
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("config.toml");
        let vault = PathBuf::from("/elsewhere/vault");
        let cfg =
            Config::load_from(ConfigSource::Default, &missing, Some(vault.clone()), None).unwrap();
        assert_eq!(cfg.vault.root, vault);
        assert_eq!(cfg.template.path, PathBuf::from(DEFAULT_TEMPLATE_PATH));
    }

    #[test]
    fn missing_config_file_without_vault_is_onboarding_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("config.toml");
        let err = Config::load_from(ConfigSource::Default, &missing, None, None).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("no config file at"), "names path: {msg}");
        assert!(
            msg.contains("[vault]") && msg.contains("OCLI_VAULT"),
            "hint shows example and alternatives: {msg}"
        );
    }

    #[test]
    fn env_override_config_missing_is_terminal_even_with_vault() {
        // A typo'd OCLI_CONFIG must not be masked by a vault from elsewhere.
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("typo.toml");
        let err = Config::load_from(
            ConfigSource::EnvOverride,
            &missing,
            Some(PathBuf::from("/v")),
            None,
        )
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("OCLI_CONFIG points to"),
            "names the env var: {err}"
        );
    }

    #[test]
    fn vault_precedence_is_override_env_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            format!("[vault]\nroot = \"{}\"\n", dir.path().display()),
        )
        .unwrap();

        // env beats file.
        let cfg = Config::load_from(
            ConfigSource::Default,
            &path,
            None,
            Some(PathBuf::from("/env/vault")),
        )
        .unwrap();
        assert_eq!(cfg.vault.root, PathBuf::from("/env/vault"));

        // override beats env.
        let cfg = Config::load_from(
            ConfigSource::Default,
            &path,
            Some(PathBuf::from("/cli/vault")),
            Some(PathBuf::from("/env/vault")),
        )
        .unwrap();
        assert_eq!(cfg.vault.root, PathBuf::from("/cli/vault"));
    }

    #[test]
    fn template_table_defaults_and_overrides() {
        // Absent table -> default path, empty values.
        let cfg = valid_minimal().validate().unwrap();
        assert_eq!(cfg.template.path, PathBuf::from(DEFAULT_TEMPLATE_PATH));
        assert!(cfg.template.values.is_empty());

        // Explicit values override; statics are literal text.
        let cfg = parse(
            r#"
            [vault]
            root = "/home/me/vault"

            [template]
            path = "custom/Feature.md"

            [template.values]
            SprintNumber = "2026.1"
            Labels = "{{DATE:YYYY}}.{{VALUE:SprintNumber}}"
        "#,
        )
        .validate()
        .unwrap();
        assert_eq!(cfg.template.path, PathBuf::from("custom/Feature.md"));
        assert_eq!(
            cfg.template.values.get("SprintNumber").map(String::as_str),
            Some("2026.1")
        );
        // Literal text passes through untouched (D22: no interpretation).
        assert_eq!(
            cfg.template.values.get("Labels").map(String::as_str),
            Some("{{DATE:YYYY}}.{{VALUE:SprintNumber}}")
        );
    }

    #[test]
    fn template_values_reject_non_scalars() {
        // D22: scalar values only. A non-scalar is a parse error - the
        // fail-loud grammar boundary.
        let err = toml::from_str::<ConfigFile>(
            r#"
            [vault]
            root = "/v"

            [template.values]
            Bad = [1, 2]
        "#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Bad"), "names the key: {err}");
    }

    // ---- path resolution --------------------------------------------------

    #[test]
    fn default_config_path_ends_with_ocli_config_toml() {
        let path = default_config_path().unwrap();
        assert_eq!(path.file_name().unwrap(), "config.toml");
        assert!(path.components().any(|c| c.as_os_str() == "ocli"));
    }
}
