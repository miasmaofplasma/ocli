use std::ops::Range;

use serde::Deserialize;

#[cfg(test)]
use crate::vault::error_chain;
use crate::{
    ftypes::{Field, render_field},
    status::Status,
    vault::{VaultError, markdown::Span},
};

/// A read-only projection of the frontmatter fields ocli consumes — never
/// a mirror of the note's schema. Unknown fields are ignored (D5: they are
/// preserved by the write path's byte-preservation, not by this model).
#[derive(Debug, Deserialize, PartialEq)]
pub struct Frontmatter {
    pub description: Option<String>,
    pub status: Option<Status>,
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

pub fn inner_yaml(fm: &str) -> &str {
    let inner = fm.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
    inner
        .strip_suffix("---\r\n")
        .or_else(|| inner.strip_suffix("---\n"))
        .or_else(|| inner.strip_suffix("---"))
        .unwrap_or(inner)
}

/// Rebuilds the document with the frontmatter's inner YAML (between the
/// `---` delimiters) replaced by `inner`. The delimiters and their line
/// endings are preserved exactly as [`markdown::parse`] found them, so
/// the D11 byte-preservation contract holds for everything outside the
/// edited inner region. The inverse of [`inner_yaml`].
pub fn splice_inner(text: &str, span: Span, inner: &str) -> String {
    let fm = &text[span.start..span.end];
    // The opening delimiter line, terminator included.
    let open_end = fm.find('\n').map_or(fm.len(), |i| i + 1);
    // The closing delimiter keeps its own terminator (LF, CRLF, or none
    // at EOF).
    let close_ending = line_ending_before(fm);

    let mut out = String::with_capacity(text.len() + inner.len());
    out.push_str(&text[..span.start]);
    out.push_str(&fm[..open_end]);
    out.push_str(inner);
    out.push_str("---");
    out.push_str(close_ending);
    out.push_str(&text[span.end..]);
    out
}

/// Validates a field's value for its emission type (D18): strings are
/// single-line, olinks are a bare name or a full `[[path|alias]]` link —
/// anything with unbalanced brackets is rejected before it can become a
/// mangled wikilink.
fn validate_field(name: &str, field: &Field) -> Result<(), VaultError> {
    let invalid = |reason: &str| VaultError::InvalidFieldValue {
        field: name.to_string(),
        reason: reason.to_string(),
    };
    match field {
        Field::Str(value) => {
            if value.contains(['\n', '\r', '\0']) {
                return Err(invalid("strings must be a single line"));
            }
        }
        Field::Olink(value) => {
            if value.contains(['\n', '\r', '\0']) {
                return Err(invalid("olink values must be a single line"));
            }
            let bare = !value.contains(['[', ']']);
            let full = value.starts_with("[[") && value.ends_with("]]");
            if !bare && !full {
                return Err(invalid(
                    "an olink is a bare name or a full [[path|alias]] link",
                ));
            }
        }
        // Bools/numbers are typed at construction — nothing to validate.
        Field::Bool(_) | Field::Int(_) | Field::Float(_) => {}
        Field::List(items) => {
            for item in items {
                validate_field(name, item)?;
            }
        }
    }
    Ok(())
}

/// The byte range of one field's region in the *inner* frontmatter: the
/// top-level `{name}:` line plus its continuation lines (indented lines
/// and `- ` list items), ending at the next key-shaped line, a blank
/// line, or the end of the text. `None` when there is no such field.
fn field_region(fm: &str, name: &str) -> Option<Range<usize>> {
    let key_prefix = format!("{name}:");
    let mut start = None;
    let mut offset = 0;
    for line in fm.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        if let Some(begin) = start {
            let is_blank = content.trim().is_empty();
            let is_continuation = content.starts_with([' ', '\t']) || content.starts_with("- ");
            if is_blank || !is_continuation {
                return Some(begin..offset);
            }
        } else if content.starts_with(&key_prefix) {
            start = Some(offset);
        }
        offset += line.len();
    }
    start.map(|s| s..fm.len())
}

/// The line ending that terminated the text before `end`, so replaced
/// regions keep the file's own endings (LF vs CRLF).
fn line_ending_before(prefix: &str) -> &'static str {
    if prefix.ends_with("\r\n") {
        "\r\n"
    } else if prefix.ends_with('\n') {
        "\n"
    } else {
        ""
    }
}

/// Sets a frontmatter field: replaces the field's region (the D11 splice
/// unit — key line through continuation lines) with `name: <emitted>`.
/// **Errors when the field doesn't exist** — for manually typed field
/// names (the `fm` command), a misspelling must not become a junk field;
/// the error carries a closest-key suggestion. Callers that *mean* to add
/// a missing field use [`append_field`].
///
/// Operates on the *inner* frontmatter text (between the `---`
/// delimiters); the caller splices it back by span. Pure text-in/text-out
/// so Phase 6's `status`/`fm` reuse it inside the D11 write path.
pub fn set_field(fm: &str, name: &str, field: &Field) -> Result<String, VaultError> {
    validate_field(name, field)?;

    let Some(region) = field_region(fm, name) else {
        return Err(VaultError::FieldNotFound {
            field: name.to_string(),
            suggestion: closest_key(fm, name),
        });
    };

    let line_ending = line_ending_before(&fm[..region.end]);
    let mut out = String::with_capacity(fm.len());
    out.push_str(&fm[..region.start]);
    out.push_str(&render_field(name, field, line_ending));
    out.push_str(line_ending);
    out.push_str(&fm[region.end..]);
    Ok(out)
}

/// Appends a field at the end of the inner frontmatter. The caller must
/// ensure the field is absent (e.g. after [`set_field`]'s
/// `FieldNotFound`) — appending an existing key would produce invalid
/// YAML (duplicate keys).
pub fn append_field(fm: &str, name: &str, field: &Field) -> Result<String, VaultError> {
    validate_field(name, field)?;

    let mut out = fm.to_string();
    let eol = if out.ends_with("\r\n") { "\r\n" } else { "\n" };
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(eol);
    }
    out.push_str(&render_field(name, field, eol));
    out.push_str(eol);
    Ok(out)
}

/// Sets the field when it exists, appends it at the end when it doesn't —
/// the convenience for callers whose missing field is *not* an error
/// (e.g. `new`'s fills, where the template is the drift source and a
/// missing field is worth a warning, not a blocker). The append emits a
/// stderr warning (D26); `fm` — where a manually typed field name must
/// not become a junk field — uses strict [`set_field`] instead.
pub fn set_or_append_field(fm: &str, name: &str, field: &Field) -> Result<String, VaultError> {
    match set_field(fm, name, field) {
        Ok(fm) => Ok(fm),
        Err(err) => match err {
            VaultError::FieldNotFound { .. } => {
                tracing::warn!(
                    field = %name,
                    "field not in frontmatter; appending it at the end"
                );
                append_field(fm, name, field)
            }
            _ => Err(err),
        },
    }
}

/// The closest existing top-level key to a missing field name, for the
/// `FieldNotFound` suggestion — case-insensitive edit distance ≤ 2.
fn closest_key(fm: &str, name: &str) -> Option<String> {
    let needle = name.to_lowercase();
    fm.split_inclusive('\n')
        .filter_map(|line| {
            let content = line.strip_suffix('\n').unwrap_or(line);
            (!content.starts_with([' ', '\t', '-'])).then(|| {
                content
                    .split_once(':')
                    .filter(|(_, rest)| !rest.starts_with(':'))
                    .map(|(key, _)| key.trim())
            })
        })
        .flatten()
        .filter(|key| !key.is_empty())
        .filter(|key| edit_distance(&needle, &key.to_lowercase()) <= 2)
        .min_by_key(|key| edit_distance(&needle, &key.to_lowercase()))
        .map(String::from)
}

/// Classic bounded Levenshtein over chars — small inputs, no need for a
/// dependency or a fancy algorithm.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
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

    /// `set_field` is strict — a missing field errors (for the manually
    /// typed `fm` command, a misspelling must not become a junk field) —
    /// and the error suggests the closest existing key.
    #[test]
    fn missing_field_errors_with_a_suggestion() {
        let fm = "estimate:\nstatus: Backlog\n";

        let err = set_field(fm, "estmate", &Field::Str("3".into())).unwrap_err();

        assert!(matches!(err, VaultError::FieldNotFound { .. }));
        let message = err.to_string();
        assert!(
            message.contains("estimate"),
            "suggests the closest key: {message}"
        );
    }

    /// `append_field` is the deliberate add: `new`'s fills reach it after
    /// `set_field`'s `FieldNotFound` (with the template-drift warning at
    /// the caller), Phase 6's `fm` never does — its typo protection *is*
    /// the error.
    #[test]
    fn append_field_adds_the_field_at_the_end() {
        let out = append_field("status: Backlog\n", "estimate", &Field::Str("3".into())).unwrap();

        assert_eq!(out, "status: Backlog\nestimate: \"3\"\n");
    }

    /// append_field is line-ending aware: a CRLF frontmatter keeps CRLF.
    #[test]
    fn append_field_keeps_crlf_endings() {
        let out = append_field("status: Backlog\r\n", "estimate", &Field::Str("3".into())).unwrap();

        assert_eq!(out, "status: Backlog\r\nestimate: \"3\"\r\n");
    }

    #[test]
    fn empty_value_is_none_but_quoted_empty_is_some() {
        // Null vs empty-string is a String-field property; status is now
        // a closed enum, so this exercises `description` instead.
        let fm = deserialize_frontmatter("description:\n").unwrap();
        assert_eq!(fm.description, None, "YAML null → None");

        let fm = deserialize_frontmatter("description: \"\"\n").unwrap();
        assert_eq!(
            fm.description,
            Some(String::new()),
            "quoted empty is a real value"
        );
    }

    #[test]
    fn plain_scalar_coerces_to_string() {
        // Same coercion, on the still-String `description` field.
        let fm = deserialize_frontmatter("description: 42\n").unwrap();
        assert_eq!(fm.description, Some("42".to_string()));
    }

    #[test]
    fn unknown_status_word_becomes_unknown() {
        // The read projection is faithful: a status outside the vocabulary
        // loads as `Unknown`, not a load failure (write contract: "read
        // anything"). `list` shows it verbatim; only `--status` rejects it.
        let fm = deserialize_frontmatter("status: banana\n").unwrap();
        assert_eq!(fm.status, Some(Status::Unknown("banana".to_string())));
    }

    #[test]
    fn wrong_typed_field_errors_naming_the_field() {
        // The `list` skip-and-warn policy keys off this error naming the
        // field — via the source chain (Display is cause-free by design).
        let err = deserialize_frontmatter("status: [a, b]\n").unwrap_err();
        assert!(error_chain(&err).contains("status"), "got: {err}");
    }

    #[test]
    fn yaml_12_bools_only() {
        assert_eq!(
            deserialize_frontmatter("done: true\n").unwrap().done,
            Some(true)
        );

        // Unquoted `yes` is a YAML 1.1 bool, not 1.2 — a string here.
        let err = deserialize_frontmatter("done: yes\n").unwrap_err();
        assert!(error_chain(&err).contains("done"), "got: {err}");
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
                status: Some(Status::InProgress),
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
    // --- set_field ------------------------------------------------------

    /// Region replacement is surgical: other fields' bytes are untouched.
    #[test]
    fn replaces_a_scalar_field_in_place() {
        let fm = "Created: 2026-09-05 10:30\ndescription:\nrepo:\nstatus: Backlog\n";

        let out = set_field(fm, "repo", &Field::Olink("connected-module-item-api".into())).unwrap();

        assert_eq!(
            out,
            "Created: 2026-09-05 10:30\ndescription:\nrepo: \"[[connected-module-item-api]]\"\nstatus: Backlog\n"
        );
    }

    /// The locator consumes continuation lines: a block-list field replaced
    /// by a scalar line leaves no residue (the case Phase 6's `fm` will
    /// exercise on real notes).
    #[test]
    fn replaces_a_block_region_with_the_scalar_line() {
        let fm = "tags:\n  - feature\n  - codewaves\nstatus: Backlog\n";

        let out = set_field(fm, "tags", &Field::Str("x".into())).unwrap();

        assert_eq!(out, "tags: \"x\"\nstatus: Backlog\n");
    }

    /// A blank line belongs to neither field — replacement must not
    /// swallow it (byte preservation of untouched formatting).
    #[test]
    fn blank_line_terminates_the_region() {
        let fm = "status: Backlog\n\nrepo:\n";

        let out = set_field(fm, "status", &Field::Str("Done".into())).unwrap();

        assert_eq!(out, "status: \"Done\"\n\nrepo:\n");
    }

    /// D18: bare olink names are auto-wrapped and always quoted; full
    /// `[[path|alias]]` links pass through as-is.
    #[test]
    fn olink_wraps_bare_names_and_keeps_full_links() {
        let out = set_field(
            "repo:\n",
            "repo",
            &Field::Olink("connected-module-item-api".into()),
        )
        .unwrap();
        assert_eq!(out, "repo: \"[[connected-module-item-api]]\"\n");

        let out = set_field("repo:\n", "repo", &Field::Olink("[[path/Ada|Ada]]".into())).unwrap();
        assert_eq!(out, "repo: \"[[path/Ada|Ada]]\"\n");
    }

    /// D18: unbalanced brackets are rejected, not wrapped into mangled
    /// wikilinks.
    #[test]
    fn olink_with_unbalanced_brackets_is_rejected() {
        let err = set_field("repo:\n", "repo", &Field::Olink("a]b".into())).unwrap_err();

        assert!(matches!(err, VaultError::InvalidFieldValue { .. }));
    }

    /// Strings must be single-line: a newline would break the
    /// one-field-per-line geometry the write path relies on.
    #[test]
    fn str_with_newline_is_rejected() {
        let err =
            set_field("description:\n", "description", &Field::Str("a\nb".into())).unwrap_err();

        assert!(matches!(err, VaultError::InvalidFieldValue { .. }));
    }

    /// Emission escapes YAML-special characters: backslash first, then
    /// the quote — the order matters and the test pins it.
    #[test]
    fn str_escapes_backslash_then_quote() {
        let out = set_field(
            "description:\n",
            "description",
            &Field::Str("a\"b\\c".into()),
        )
        .unwrap();

        assert_eq!(out, "description: \"a\\\"b\\\\c\"\n");
    }

    /// The wrapper replaces when the field exists — identical to
    /// `set_field`'s strict path.
    #[test]
    fn set_or_append_replaces_when_present() {
        let fm = "status: Backlog\nrepo:\n";

        let out = set_or_append_field(fm, "status", &Field::Str("Done".into())).unwrap();

        assert_eq!(out, "status: \"Done\"\nrepo:\n");
    }

    /// The wrapper appends when the field is missing (with the drift
    /// warning on stderr) — `new`'s fills compose through this instead of
    /// matching on `FieldNotFound` themselves.
    #[test]
    fn set_or_append_appends_when_missing() {
        let out =
            set_or_append_field("status: Backlog\n", "estimate", &Field::Str("3".into())).unwrap();

        assert_eq!(out, "status: Backlog\nestimate: \"3\"\n");
    }

    /// The splice round-trips: inner_yaml out, an edit, splice_inner back —
    /// delimiters and the body untouched, byte for byte.
    #[test]
    fn splice_inner_round_trips_lf_documents() {
        let text = "---\nstatus: Backlog\n---\n# body\n";
        let document = crate::vault::markdown::parse(text);
        let span = document.frontmatter().unwrap();

        let inner = inner_yaml(&text[span]);
        let edited = set_field(inner, "status", &Field::Str("Done".into())).unwrap();
        let out = splice_inner(text, span, &edited);

        assert_eq!(out, "---\nstatus: \"Done\"\n---\n# body\n");
    }

    /// CRLF documents keep CRLF delimiters through the splice.
    #[test]
    fn splice_inner_preserves_crlf_delimiters() {
        let text = "---\r\nstatus: Backlog\r\n---\r\n# body\r\n";
        let document = crate::vault::markdown::parse(text);
        let span = document.frontmatter().unwrap();

        let inner = inner_yaml(&text[span]);
        let edited = set_field(inner, "status", &Field::Str("Done".into())).unwrap();
        let out = splice_inner(text, span, &edited);

        assert_eq!(out, "---\r\nstatus: \"Done\"\r\n---\r\n# body\r\n");
    }
}
