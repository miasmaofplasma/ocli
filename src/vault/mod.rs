use std::collections::HashMap;

use regex::Regex;

pub mod edit;
pub mod features;
pub mod frontmatter;
pub mod markdown;
pub mod note;
pub mod template;

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
    #[error("could not find template at {path}")]
    TemplateNotFound {
        path: String,
        #[source]
        error: std::io::Error,
    },
    #[error("could not find front matter for template at {path}")]
    FrontmatterNotFound { path: String },
    #[error("could not read note at {path}")]
    IoError {
        path: String,
        #[source]
        error: std::io::Error,
    },
    #[error("note already exists at {path}")]
    NoteAlreadyExists {
        path: String,
        #[source]
        error: std::io::Error,
    },
    #[error("could not find current branch")]
    CurrentBranchNotFound,
    #[error("could not parse branch {branch} with regex {pattern}")]
    CouldNotParseBranchName { branch: String, pattern: String },
    #[error(
        "could not compose the note name for branch {branch}: {name:?} still has unresolved placeholders"
    )]
    CouldNotComposeNoteName { branch: String, name: String },
    #[error(
        "note name {name:?} from branch {branch:?} is not filename-safe: it must be non-empty, not start with '.', and contain no '/', NUL, or newline"
    )]
    UnsafeNoteName { branch: String, name: String },
    #[error(
        "frontmatter has no field {field:?}{}",
        suggestion
            .as_deref()
            .map(|s| format!("; did you mean {s:?}?"))
            .unwrap_or_default()
    )]
    FieldNotFound {
        field: String,
        suggestion: Option<String>,
    },
    #[error("invalid value for frontmatter field {field:?}: {reason}")]
    InvalidFieldValue { field: String, reason: String },
    #[error(
        "template placeholder {name:?} on line {line} has no value; layers are --set > key argument > branch captures > [template.values]"
    )]
    PlaceholderWithoutValue { name: String, line: usize },
    #[error(
        "unsupported {{{{DATE:{arg}}}}} on line {line}; the supported format is exactly {{{{DATE:YYYY-MM-DD HH:mm}}}}"
    )]
    UnsupportedDateFormat { arg: String, line: usize },
    #[error(
        "unsupported template construct {construct:?} on line {line}; the grammar is {{{{VALUE:name}}}} and {{{{DATE:YYYY-MM-DD HH:mm}}}}"
    )]
    UnsupportedConstruct { construct: String, line: usize },
    #[error("could not find path at {path}")]
    PathNotFound { path: String },
    /// The optimistic-retry bound (D11) ran out: the note kept changing
    /// between read and write — a concurrent editor is very active.
    #[error("the note at {path} kept changing while editing; try again")]
    NoteChangedUnderUs { path: String },
}

/// Matches `branch` against the ticket pattern once — the D20b hard
/// require's single regex pass, whose `Captures` feed both the value
/// map ([`capture_map`]) and the id composition ([`note_name`]), so the
/// same text is never matched twice.
///
/// A non-matching branch → `CouldNotParseBranchName` naming branch and
/// pattern (the D20a/D20b signal: `new` refuses off-pattern branches
/// because a misfiled note is the expensive mistake).
pub fn branch_captures<'a>(
    branch: &'a str,
    regex: &Regex,
) -> Result<regex::Captures<'a>, VaultError> {
    regex
        .captures(branch)
        .ok_or_else(|| VaultError::CouldNotParseBranchName {
            branch: branch.to_string(),
            pattern: regex.to_string(),
        })
}

/// Every named capture group that participated (D21): `BCP-14392` →
/// `{"FeatureType": "BCP", "TicketNumber": "14392"}`. This is the map
/// the value-map's Branch layer is built from — extraction is not
/// filtered by the id template, so groups consumed only by
/// `{{VALUE:...}}` placeholders arrive too. A named group that did not
/// participate (optional in the pattern) is omitted: the map models
/// what the branch actually captured, and a `{{VALUE:...}}` depending
/// on it fails later with the missing-placeholder error naming it.
pub fn capture_map(regex: &Regex, caps: &regex::Captures<'_>) -> HashMap<String, String> {
    regex
        .capture_names()
        .flatten()
        .filter_map(|name| {
            caps.name(name)
                .map(|mat| (name.to_string(), mat.as_str().to_string()))
        })
        .collect()
}

/// The note name (ticket ID) for a branch: the `[tickets] id` template
/// composed over the branch's captures (D25) — composed faithfully, no
/// format rule (the team's pattern + id template own the shape; revises
/// D20's argument-path validation).
///
/// Two gates: substitution integrity (a surviving `{...}` means an
/// optional group didn't participate → `CouldNotComposeNoteName`) and
/// filename safety ([`ensure_safe_note_name`]).
pub fn note_name(
    branch: &str,
    regex: &Regex,
    caps: &regex::Captures<'_>,
    ticket_pattern: &str,
) -> Result<String, VaultError> {
    // Compose faithfully: every named capture replaces its `{name}` in
    // the template; the template keeps its own literal text otherwise.
    let mut name = ticket_pattern.to_string();
    for group in regex.capture_names().flatten() {
        if let Some(value) = caps.name(group) {
            name = name.replace(&format!("{{{group}}}"), value.as_str());
        }
    }

    // Gate 1 — substitution integrity (D12-style fail-loud,
    // format-neutral): a surviving `{...}` means a referenced group
    // didn't participate.
    if name.contains('{') {
        return Err(VaultError::CouldNotComposeNoteName {
            branch: branch.to_string(),
            name,
        });
    }

    ensure_safe_note_name(branch, &name)?;
    Ok(name)
}

/// Gate 2 — filename safety, not format taste: the note must be ONE
/// visible file in `features/` (the scan skips dotfiles and isn't
/// recursive, so these constraints are machinery, not convention).
/// Applied to the composed id *and* to an explicit key argument —
/// `ocli new ../evil` must not escape the features directory.
pub fn ensure_safe_note_name(branch: &str, name: &str) -> Result<(), VaultError> {
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\0', '\n']) {
        return Err(VaultError::UnsafeNoteName {
            branch: branch.to_string(),
            name: name.to_string(),
        });
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use regex::Regex;

    use crate::vault::{
        VaultError, branch_captures, capture_map, ensure_safe_note_name, note_name,
    };

    /// The default `[tickets]` branch pattern (D20):
    /// `^(?<FeatureType>[A-Z]+)-(?<TicketNumber>\d+)`
    fn default_pattern() -> Regex {
        Regex::new(r"^(?<FeatureType>[A-Z]+)-(?<TicketNumber>\d+)").unwrap()
    }

    /// Every named group of the pattern lands in the map — the D21
    /// contract the first implementation missed by filtering on the id
    /// template.
    #[test]
    fn extracts_all_named_capture_groups() {
        let regex = default_pattern();
        let caps = branch_captures("BCP-14392", &regex).unwrap();
        let captures = capture_map(&regex, &caps);

        assert_eq!(captures.get("FeatureType"), Some(&"BCP".to_string()));
        assert_eq!(captures.get("TicketNumber"), Some(&"14392".to_string()));
        assert_eq!(captures.len(), 2);
    }

    /// The pattern is anchored at the start only (D20): everything after
    /// the ticket key is ordinary branch naming, and the captures still
    /// extract.
    #[test]
    fn extracts_from_branches_with_suffixes() {
        let regex = default_pattern();
        let caps = branch_captures("BCP-14392-fix-login", &regex).unwrap();
        let captures = capture_map(&regex, &caps);

        assert_eq!(captures.get("FeatureType"), Some(&"BCP".to_string()));
        assert_eq!(captures.get("TicketNumber"), Some(&"14392".to_string()));
    }

    /// D21: groups the id template never references still feed the value
    /// map — a `{{VALUE:Epic}}` placeholder resolves from the branch alone.
    #[test]
    fn extracts_captures_beyond_the_id_templates_references() {
        let regex =
            Regex::new(r"^(?<FeatureType>[A-Z]+)-(?<TicketNumber>\d+)-(?<Epic>\w+)").unwrap();

        let caps = branch_captures("BCP-14392-golden", &regex).unwrap();
        let captures = capture_map(&regex, &caps);

        assert_eq!(captures.get("Epic"), Some(&"golden".to_string()));
        assert_eq!(captures.len(), 3);
    }

    /// A non-participating optional group is omitted, not an error: the
    /// map models what the branch captured, and a `{{VALUE:...}}` that
    /// needs it fails later with D21's missing-placeholder error naming
    /// the placeholder.
    #[test]
    fn omits_optional_groups_that_do_not_participate() {
        let regex =
            Regex::new(r"^(?<FeatureType>[A-Z]+)-(?<TicketNumber>\d+)(?:-(?<Epic>\w+))?").unwrap();

        let caps = branch_captures("BCP-14392", &regex).unwrap();
        let captures = capture_map(&regex, &caps);

        assert_eq!(captures.get("FeatureType"), Some(&"BCP".to_string()));
        assert!(!captures.contains_key("Epic"));
    }

    /// A non-matching branch is the D20a/D20b signal: the error names both
    /// inputs so the caller can point at the pattern (D26/D32: errors name
    /// the operation's inputs, causes travel separately).
    #[test]
    fn non_matching_branch_errors_naming_branch_and_pattern() {
        let err = branch_captures("main", &default_pattern()).unwrap_err();

        assert!(matches!(err, VaultError::CouldNotParseBranchName { .. }));
        let message = err.to_string();
        assert!(message.contains("main"), "names the branch: {message}");
        assert!(
            message.contains("TicketNumber"),
            "names the pattern: {message}"
        );
    }

    /// The id template composes faithfully — Goal 3: the config owns the
    /// shape, so `"Ticket-{TicketNumber}"` is as legal as the default.
    #[test]
    fn composes_custom_id_templates_as_configured() {
        let regex = default_pattern();
        let caps = branch_captures("BCP-14392-fix-login", &regex).unwrap();

        let name = note_name(
            "BCP-14392-fix-login",
            &regex,
            &caps,
            "Ticket-{TicketNumber}",
        )
        .unwrap();

        assert_eq!(name, "Ticket-14392");
    }

    /// Gate 1: an optional group that didn't participate leaves `{...}`
    /// behind — the error shows the mangled composition so the pattern is
    /// debuggable.
    #[test]
    fn unresolved_placeholder_errors_naming_the_mangled_name() {
        let regex =
            Regex::new(r"^(?<FeatureType>[A-Z]+)-(?<TicketNumber>\d+)(?:-(?<Epic>\w+))?").unwrap();
        let caps = branch_captures("BCP-14392", &regex).unwrap();

        let err = note_name(
            "BCP-14392",
            &regex,
            &caps,
            "{FeatureType}-{TicketNumber}-{Epic}",
        )
        .unwrap_err();

        assert!(matches!(err, VaultError::CouldNotComposeNoteName { .. }));
        let message = err.to_string();
        assert!(
            message.contains("{Epic}"),
            "shows what failed to resolve: {message}"
        );
    }

    /// Gate 2: the note must be ONE visible file in features/ — a captured
    /// `/` would nest the file out of the non-recursive scan.
    #[test]
    fn unsafe_note_name_errors() {
        let regex = Regex::new(r"^(?<Id>[A-Z]+/\d+)").unwrap();
        let caps = branch_captures("BCP/14392", &regex).unwrap();

        let err = note_name("BCP/14392", &regex, &caps, "{Id}").unwrap_err();

        assert!(matches!(err, VaultError::UnsafeNoteName { .. }));
    }

    /// Gate 2 applies to the explicit key argument too (D25): `ocli new`
    /// must not be able to name a file outside `features/`.
    #[test]
    fn unsafe_explicit_key_errors() {
        let err = ensure_safe_note_name("BCP-14392-fix-login", "../evil").unwrap_err();

        assert!(matches!(err, VaultError::UnsafeNoteName { .. }));
        let err = ensure_safe_note_name("BCP-14392-fix-login", ".hidden").unwrap_err();
        assert!(matches!(err, VaultError::UnsafeNoteName { .. }));
        let err = ensure_safe_note_name("BCP-14392-fix-login", "").unwrap_err();
        assert!(matches!(err, VaultError::UnsafeNoteName { .. }));
    }
}
