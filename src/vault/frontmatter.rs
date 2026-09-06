use serde::Deserialize;

use crate::vault::VaultError;

/// A read-only projection of the frontmatter fields ocli consumes — never
/// a mirror of the note's schema. Unknown fields are ignored (D5: they are
/// preserved by the write path's byte-preservation, not by this model).
#[derive(Debug, Deserialize, PartialEq)]
pub struct Frontmatter {
    pub description: Option<String>,
    pub status: Option<String>,
    pub repo: Option<String>,
    pub done: Option<bool>,
}

/// Deserializes the *inner* frontmatter YAML: the text between the `---`
/// delimiter lines, which the caller strips (delimiter geometry is
/// `markdown::Document`'s business). Empty, blank, or comment-only inner
/// text is valid and yields all-`None` (verified against yaml_serde).
pub fn deserialize_frontmatter(fm: &str) -> Result<Frontmatter, VaultError> {
    yaml_serde::from_str::<Frontmatter>(fm).map_err(VaultError::CouldNotDeserializeFrontmatter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_none() -> Frontmatter {
        Frontmatter {
            description: None,
            status: None,
            repo: None,
            done: None,
        }
    }

    #[test]
    fn empty_inner_text_is_all_none() {
        // The `---\n---\n` block's inner text is empty; yaml_serde accepts it.
        assert_eq!(deserialize_frontmatter("").unwrap(), all_none());
    }

    #[test]
    fn blank_and_comment_only_inner_is_all_none() {
        assert_eq!(deserialize_frontmatter("  \n").unwrap(), all_none());
        assert_eq!(
            deserialize_frontmatter("# just a comment\n").unwrap(),
            all_none()
        );
    }

    #[test]
    fn empty_value_is_none_but_quoted_empty_is_some() {
        let fm = deserialize_frontmatter("status:\n").unwrap();
        assert_eq!(fm.status, None, "YAML null → None");

        let fm = deserialize_frontmatter("status: \"\"\n").unwrap();
        assert_eq!(
            fm.status,
            Some(String::new()),
            "quoted empty is a real value"
        );
    }

    #[test]
    fn plain_scalar_coerces_to_string() {
        let fm = deserialize_frontmatter("status: 42\n").unwrap();
        assert_eq!(fm.status, Some("42".to_string()));
    }

    #[test]
    fn wrong_typed_field_errors_naming_the_field() {
        // The `list` skip-and-warn policy keys off this error naming the field.
        let err = deserialize_frontmatter("status: [a, b]\n").unwrap_err();
        assert!(err.to_string().contains("status"), "got: {err}");
    }

    #[test]
    fn yaml_12_bools_only() {
        assert_eq!(
            deserialize_frontmatter("done: true\n").unwrap().done,
            Some(true)
        );

        // Unquoted `yes` is a YAML 1.1 bool, not 1.2 — a string here.
        let err = deserialize_frontmatter("done: yes\n").unwrap_err();
        assert!(err.to_string().contains("done"), "got: {err}");
    }

    #[test]
    fn unknown_fields_are_ignored() {
        // Unquoted YAML timestamp, sequences, anything ocli doesn't consume.
        let fm = deserialize_frontmatter(
            "Created: 2026-09-05 10:30\ntags:\n  - feature\naliases:\n  - BCP-74043\nstatus: In Progress\n",
        )
        .unwrap();
        assert_eq!(
            fm,
            Frontmatter {
                description: None,
                status: Some("In Progress".to_string()),
                repo: None,
                done: None,
            }
        );
    }

    #[test]
    fn real_template_frontmatter_parses() {
        // Verbatim inner text of the user's QuickAdd Feature template.
        let fm = deserialize_frontmatter(
            "Created: \"{{DATE:YYYY-MM-DD HH:mm}}\"\n\
             aliases:\n\
             \x20 - \"{{VALUE:FeatureType}}-{{VALUE:TicketNumber}}\"\n\
             type: \"{{VALUE:FeatureType}}\"\n\
             description:\n\
             owner:\n\
             epic:\n\
             relates-to:\n\
             blocked-by:\n\
             sprint:\n\
             repo:\n\
             jira: https://bddevops.atlassian.net/browse/{{VALUE:FeatureType}}-{{VALUE:TicketNumber}}\n\
             pr:\n\
             estimate:\n\
             status:\n\
             done: false\n\
             tags:\n\
             \x20 - feature\n\
             \x20 - codewaves\n",
        )
        .unwrap();
        assert_eq!(fm.done, Some(false));
        assert_eq!(fm.status, None);
        assert_eq!(fm.repo, None);
    }
}
