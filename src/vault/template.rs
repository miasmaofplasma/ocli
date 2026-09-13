//! The QuickAdd template renderer (D12, Q8): a pure text-in/text-out leaf.
//! It knows nothing about git, config, or the CLI — `commands::new` feeds
//! it a [`ValueMap`] and a clock reading, and gets rendered text back.
//!
//! Grammar (exact, fail-loud — anything else is a hard error naming the
//! line, so an unknown QuickAdd or Templater construct can never silently
//! pass through into a created note):
//!
//! - `{{VALUE:name}}` — replaced by the value map's `name` entry
//!   (missing → error naming the placeholder, D21)
//! - `{{DATE:YYYY-MM-DD HH:mm}}` — replaced by the local timestamp in the
//!   one house format. The argument is matched exactly, never parsed:
//!   any other argument → error naming the line and the supported format.
//!
//! Substitution rebuilds the text with untouched bytes verbatim — no YAML
//! parse, no serialization (Q8: the parse→mutate→serialize route rewrites
//! quote style on real templates and passes unknown placeholders
//! silently; rejected on evidence, see CURRENT.md).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Local, Timelike};

use crate::vault::VaultError;

/// The one house date format (D19/D12): the exact `{{DATE:...}}` argument
/// the renderer accepts, and the format it emits.
pub const DATE_FORMAT: &str = "YYYY-MM-DD HH:mm";

/// Renders `now` in the house format: integer accessors + std padding,
/// deliberately no strftime — format strings double letters into
/// literals (`%YYYY` is the year plus "YY") and this grammar matches a
/// constant, never a parsed spec.
pub fn house_format(now: &DateTime<Local>) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute()
    )
}

/// Where a value-map entry came from (Q8): kept per entry so a conflict
/// between layers is attributable. Ascending precedence (D21/D22):
/// [`Template`] < [`Branch`] < [`Key`] < [`Set`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `[template.values]` — static template defaults (D22).
    Template,
    /// Named captures from the branch pattern (D21).
    Branch,
    /// Captures from matching the pattern against the explicit key.
    Key,
    /// `--set NAME=VALUE` — the user's explicit word, highest.
    Set,
}

/// The merged value map (Q8): `BTreeMap` for deterministic iteration,
/// `(value, source)` per name for provenance. Layers are inserted in
/// ascending precedence — a later insert overwrites an earlier one.
#[derive(Debug, Default)]
pub struct ValueMap(BTreeMap<String, (String, Source)>);

impl ValueMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Overwrites any earlier layer's entry for the same name — call in
    /// ascending precedence order (Template → Branch → Key → Set).
    pub fn insert(&mut self, source: Source, name: impl Into<String>, value: impl Into<String>) {
        self.0.insert(name.into(), (value.into(), source));
    }

    /// Inserts a whole layer at once.
    pub fn insert_all(
        &mut self,
        source: Source,
        pairs: impl IntoIterator<Item = (String, String)>,
    ) {
        for (name, value) in pairs {
            self.insert(source, name, value);
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(|(value, _)| value.as_str())
    }

    /// The layer an entry came from — provenance for tests and D20c-style
    /// conflict reporting.
    pub fn source_of(&self, name: &str) -> Option<Source> {
        self.0.get(name).map(|(_, source)| *source)
    }
}

/// The loaded QuickAdd template: its text and where it came from.
pub struct FeatureTemplate {
    path: PathBuf,
    text: String,
}

impl FeatureTemplate {
    /// Reads the template, distinguishing a missing file (its own error,
    /// naming the path — D12/D26) from other IO failures.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, VaultError> {
        let path = path.as_ref();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => match error.kind() {
                std::io::ErrorKind::NotFound => {
                    return Err(VaultError::TemplateNotFound {
                        path: path.display().to_string(),
                        error,
                    });
                }
                _ => {
                    return Err(VaultError::IoError {
                        path: path.display().to_string(),
                        error,
                    });
                }
            },
        };
        Ok(Self {
            text,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Renders every placeholder in the template — frontmatter and body,
    /// one pass — against `values` and `now`.
    pub fn render(&self, values: &ValueMap, now: &DateTime<Local>) -> Result<String, VaultError> {
        substitute(&self.text, values, now)
    }
}

/// One classified `{{...}}` occurrence: the placeholder's kind and the
/// byte range it occupies in the template text (span includes the
/// `{{`...`}}` delimiters).
enum Placeholder<'a> {
    Value { name: &'a str, span: (usize, usize) },
    Date { span: (usize, usize) },
}

/// The 1-based line a byte offset falls on, for error messages that name
/// the line (D12).
fn line_of(text: &str, offset: usize) -> usize {
    1 + text[..offset].bytes().filter(|&b| b == b'\n').count()
}

/// Classifies every `{{...}}` in the text. `{{` without a `}}` on the
/// same line, `{{VALUE:}}` with an empty name, and every non-grammar
/// construct (`{{TIME:...}}`, Templater-adjacent junk) are hard errors
/// naming the line.
fn scan(text: &str) -> Result<Vec<Placeholder<'_>>, VaultError> {
    let mut out = Vec::new();
    let mut cursor = 0;

    while let Some(found) = text[cursor..].find("{{") {
        let open = cursor + found;
        let inner_start = open + "{{".len();
        let line = line_of(text, open);

        // Same-line rule: the `}}` must arrive before the next newline —
        // not necessarily at end-of-line, since two placeholders can sit
        // adjacent (`{{VALUE:A}}-{{VALUE:B}}`). A multi-line `{{...` is a
        // construct error showing the rest of the line.
        let rest = &text[inner_start..];
        let line_end = rest.find('\n').unwrap_or(rest.len());
        let first_line = &rest[..line_end];
        let Some(inner_end) = first_line.find("}}") else {
            return Err(VaultError::UnsupportedConstruct {
                construct: first_line.to_string(),
                line,
            });
        };
        let inner = &first_line[..inner_end];

        // The span covers `{{` ... `}}`, both delimiters included.
        let span = (open, inner_start + inner_end + 2);

        if let Some(name) = inner.strip_prefix("VALUE:") {
            if !name.is_empty() {
                out.push(Placeholder::Value { name, span });
            } else {
                return Err(VaultError::UnsupportedConstruct {
                    construct: inner.to_string(),
                    line,
                });
            }
        } else if let Some(arg) = inner.strip_prefix("DATE:") {
            if arg == DATE_FORMAT {
                out.push(Placeholder::Date { span });
            } else {
                return Err(VaultError::UnsupportedDateFormat {
                    arg: arg.to_string(),
                    line,
                });
            }
        } else {
            return Err(VaultError::UnsupportedConstruct {
                construct: inner.to_string(),
                line,
            });
        }

        cursor = span.1;
    }

    Ok(out)
}

/// Replaces every placeholder with its resolved value, copying all other
/// bytes verbatim — the D11 byte-preservation contract holds for
/// everything ocli did not decide to change.
pub fn substitute(
    text: &str,
    values: &ValueMap,
    now: &DateTime<Local>,
) -> Result<String, VaultError> {
    let placeholders = scan(text)?;
    let stamp = house_format(now);

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for placeholder in placeholders {
        let end = match placeholder {
            Placeholder::Value { name, span } => {
                let Some(value) = values.get(name) else {
                    return Err(VaultError::PlaceholderWithoutValue {
                        name: name.to_string(),
                        line: line_of(text, span.0),
                    });
                };
                out.push_str(&text[cursor..span.0]);
                out.push_str(value);
                span.1
            }
            Placeholder::Date { span } => {
                out.push_str(&text[cursor..span.0]);
                out.push_str(&stamp);
                span.1
            }
        };
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    /// The fixture copy of the real QuickAdd Feature template — the
    /// renderer's contract test (CURRENT.md, template-audit action item).
    const FIXTURE_TEMPLATE: &str = include_str!("../../tests/fixtures/vault/templates/Feature.md");

    fn fixed_now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 9, 12, 10, 30, 0).unwrap()
    }

    fn branch_values() -> ValueMap {
        let mut values = ValueMap::new();
        values.insert(Source::Branch, "FeatureType", "BCP");
        values.insert(Source::Branch, "TicketNumber", "14392");
        values
    }

    #[test]
    fn house_format_is_padded_local_time() {
        assert_eq!(house_format(&fixed_now()), "2026-09-12 10:30");
    }

    /// The contract test: the real template renders to exactly the real
    /// template with the three audited constructs replaced — byte for
    /// byte, quoting and Meta Bind blocks untouched.
    #[test]
    fn renders_the_real_template_byte_exactly() {
        let expected = FIXTURE_TEMPLATE
            .replace("{{DATE:YYYY-MM-DD HH:mm}}", "2026-09-12 10:30")
            .replace("{{VALUE:FeatureType}}", "BCP")
            .replace("{{VALUE:TicketNumber}}", "14392");

        let out = substitute(FIXTURE_TEMPLATE, &branch_values(), &fixed_now()).unwrap();

        assert_eq!(out, expected);
        assert!(!out.contains("{{"), "no placeholder survives");
        assert!(
            out.contains("## Descisions"),
            "the user's typo survives untouched — it is theirs to fix"
        );
    }

    /// `{{DATE:...}}` is exact-matched, never parsed: a near-miss argument
    /// errors naming the line and the one supported format.
    #[test]
    fn date_argument_is_exact_matched() {
        let text = "Created: \"{{DATE:YYYY-MM-DD}}\"\n";

        let err = substitute(text, &ValueMap::new(), &fixed_now()).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("YYYY-MM-DD}}"),
            "names the unsupported argument: {message}"
        );
        assert!(
            message.contains("YYYY-MM-DD HH:mm"),
            "names the supported format: {message}"
        );
        assert!(message.contains("line 1"), "names the line: {message}");
    }

    /// Unrecognized constructs (`{{TIME:...}}` was dropped from the
    /// grammar, D12 amended) hard-error naming the construct and line —
    /// never a silent pass-through into a created note.
    #[test]
    fn unknown_construct_errors_naming_the_line() {
        let text = "---\nstatus:\n---\ncreated at {{TIME:HH:mm}}\n";

        let err = substitute(text, &ValueMap::new(), &fixed_now()).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("TIME:HH:mm"), "names it: {message}");
        assert!(message.contains("line 4"), "names the line: {message}");
    }

    /// A `{{` with no closing `}}` on the same line is a construct error,
    /// not a swallowed prefix.
    #[test]
    fn unterminated_open_errors_showing_the_rest() {
        let text = "a\nb\n{{oops no close\n";

        let err = substitute(text, &ValueMap::new(), &fixed_now()).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("oops no close"), "shows it: {message}");
        assert!(message.contains("line 3"), "names the line: {message}");
    }

    /// A `{{VALUE:name}}` with no entry in any layer is D21's loud error
    /// naming the placeholder.
    #[test]
    fn value_without_a_layer_entry_errors_naming_it() {
        let text = "epic: \"{{VALUE:Epic}}\"\n";

        let err = substitute(text, &branch_values(), &fixed_now()).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("Epic"), "names the placeholder: {message}");
        assert!(message.contains("line 1"), "names the line: {message}");
    }

    /// Two placeholders on one line (the template's aliases line) both
    /// resolve — the scan cursor must not skip the second `{{`.
    #[test]
    fn adjacent_placeholders_on_one_line_both_resolve() {
        let text = "aliases:\n  - \"{{VALUE:FeatureType}}-{{VALUE:TicketNumber}}\"\n";

        let out = substitute(text, &branch_values(), &fixed_now()).unwrap();

        assert_eq!(out, "aliases:\n  - \"BCP-14392\"\n");
    }

    /// Layers merge with ascending precedence: later inserts overwrite —
    /// `--set` beats the key argument beats branch captures beats
    /// `[template.values]` (D21/D22).
    #[test]
    fn value_map_layers_merge_by_precedence() {
        let mut values = ValueMap::new();
        values.insert(Source::Template, "FeatureType", "Ticket");
        values.insert(Source::Branch, "FeatureType", "BCP");
        assert_eq!(values.get("FeatureType"), Some("BCP"));
        assert_eq!(values.source_of("FeatureType"), Some(Source::Branch));

        values.insert(Source::Key, "FeatureType", "NGP");
        assert_eq!(values.source_of("FeatureType"), Some(Source::Key));

        values.insert(Source::Set, "FeatureType", "XX");
        assert_eq!(values.get("FeatureType"), Some("XX"));
        assert_eq!(values.source_of("FeatureType"), Some(Source::Set));
    }
}
