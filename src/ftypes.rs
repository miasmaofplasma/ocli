//! The frontmatter field-type vocabulary (D18/D31): the one home for what
//! config *declares* (`[frontmatter]` types, [`FieldType`]) and what the
//! write path *emits` ([`Field`], the typed value `set_field` renders).
//! Split out of `config.rs` when the write path became the second
//! consumer, so the two halves of the vocabulary cannot drift apart.
//!
//! Leaf module: it imports nothing from `vault` — `vault::frontmatter`
//! consumes this, never the reverse.

use std::fmt::Display;
use std::str::FromStr;

use serde::Deserialize;
use thiserror::Error;

/// A field type as declared in config (D18): `string` | `int` | `float` |
/// `bool` | `olink` | `list<T>`.
#[derive(Debug, PartialEq, Clone)]
pub enum FieldType {
    Str,
    Int,
    Float,
    Bool,
    Olink,
    List(Box<FieldType>),
}

#[derive(Debug, Error)]
#[error("{0} is not a valid field type")]
pub struct FieldTypeError(pub String);

/// Coercion failures for [`Field::parse`]: the raw CLI value doesn't fit
/// the field's declared type (D18), or the type can't take list input.
#[derive(Debug, Error)]
pub enum FieldError {
    #[error("{value:?} is not a valid {type_name} value")]
    InvalidValue {
        type_name: &'static str,
        value: String,
    },
    #[error("nested lists (list<list<T>>) cannot be written yet")]
    NestedList,
}

impl FromStr for FieldType {
    type Err = FieldTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "string" => Ok(Self::Str),
            "int" => Ok(Self::Int),
            "float" => Ok(Self::Float),
            "bool" => Ok(Self::Bool),
            "olink" => Ok(Self::Olink),
            other => {
                let inner = other
                    .strip_prefix("list<")
                    .and_then(|r| r.strip_suffix(">"))
                    .ok_or_else(|| FieldTypeError(other.into()))?;
                Ok(Self::List(Box::new(inner.parse()?)))
            }
        }
    }
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

/// A frontmatter value with its emission type: how [`set_field`] renders
/// it into the note. The type decides the quoting (D18 emission rules),
/// so callers can't emit raw text into YAML.
///
/// [`set_field`]: crate::vault::frontmatter::set_field
#[derive(Debug, PartialEq, Clone)]
pub enum Field {
    Str(String),
    Olink(String),
    Bool(bool),
    Int(i32),
    Float(f32),
    List(Vec<Field>),
}

impl Field {
    /// Coerces a raw CLI value to the field's declared type (D18): the
    /// value-ingestion half of `fm`. Lists split on `,` and coerce each
    /// element, so input matches [`Display`]'s comma-joined human form.
    /// Validation (single-line strings, olink shape) stays with
    /// `validate_field` — this only coerces.
    pub fn parse(ty: &FieldType, raw: &str) -> Result<Self, FieldError> {
        match ty {
            FieldType::Str => Ok(Field::Str(raw.to_string())),
            FieldType::Olink => Ok(Field::Olink(raw.to_string())),
            FieldType::Int => {
                raw.parse::<i32>()
                    .map(Field::Int)
                    .map_err(|_| FieldError::InvalidValue {
                        type_name: "int",
                        value: raw.to_string(),
                    })
            }
            FieldType::Float => match raw.parse::<f32>() {
                // `inf`/`NaN` have no clean scalar in our hand-rolled
                // emitter — refuse them at the coercion boundary.
                Ok(value) if value.is_finite() => Ok(Field::Float(value)),
                _ => Err(FieldError::InvalidValue {
                    type_name: "float",
                    value: raw.to_string(),
                }),
            },
            FieldType::Bool => {
                raw.parse::<bool>()
                    .map(Field::Bool)
                    .map_err(|_| FieldError::InvalidValue {
                        type_name: "bool",
                        value: raw.to_string(),
                    })
            }
            FieldType::List(elem) => {
                if matches!(elem.as_ref(), FieldType::List(_)) {
                    return Err(FieldError::NestedList);
                }
                let items = raw
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(|item| Field::parse(elem.as_ref(), item))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Field::List(items))
            }
        }
    }
}

impl Display for Field {
    /// Human form (console echo, logs, errors) — deliberately *not* YAML:
    /// bare values, comma-joined lists. Emission into the note goes through
    /// [`render_field`], which owns the YAML layout.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Field::Str(value) | Field::Olink(value) => write!(f, "{value}"),
            Field::Bool(value) => write!(f, "{value}"),
            Field::Int(value) => write!(f, "{value}"),
            Field::Float(value) => write!(f, "{}", float_repr(*value)),
            Field::List(items) => {
                let list = items
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{list}")
            }
        }
    }
}

/// YAML emission for one top-level frontmatter field (D18 write path):
/// scalars as `name: value`, lists as block style
/// `name:\n  - item\n  - item`. `eol` is the note's own line ending
/// (LF or CRLF), so multi-line values keep D11 byte fidelity.
///
/// Separate from [`Display`] on purpose: YAML is a serialization format,
/// and byte-faithful emission is hand-rolled — Q8 rejected round-tripping
/// through `yaml_serde`'s emitter (it rewrites quote style and indent).
pub fn render_field(name: &str, field: &Field, eol: &str) -> String {
    match field {
        Field::List(items) => {
            if items.is_empty() {
                return format!("{name}: []");
            }
            let body = items
                .iter()
                .map(|item| format!("  - {}", yaml_value(item)))
                .collect::<Vec<_>>()
                .join(eol);
            format!("{name}:{eol}{body}")
        }
        scalar => format!("{name}: {}", yaml_value(scalar)),
    }
}

/// Single-line YAML for a *scalar* value: strings double-quoted and
/// escaped, olinks auto-wrapped, bools/numbers bare. `List` renders
/// flow-style (`[a, b]`) — only reachable for nested lists, which the
/// coercion layer refuses, but it's valid YAML regardless.
fn yaml_value(field: &Field) -> String {
    match field {
        Field::Str(value) => format!("\"{}\"", escape_yaml_double_quoted(value)),
        // A bare name is auto-wrapped; a full `[[path|alias]]` is kept
        // as-is (D18). `validate_field` guarantees only these two shapes.
        Field::Olink(value) if value.starts_with("[[") => format!("\"{value}\""),
        Field::Olink(value) => format!("\"[[{value}]]\""),
        Field::Bool(value) => value.to_string(),
        Field::Int(value) => value.to_string(),
        Field::Float(value) => float_repr(*value),
        Field::List(items) => {
            let body = items.iter().map(yaml_value).collect::<Vec<_>>().join(", ");
            format!("[{body}]")
        }
    }
}

/// YAML double-quoted-style escaping: backslash first, then the quote.
pub(crate) fn escape_yaml_double_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// YAML (and human) form for a float: Rust's `Display` prints `5` for
/// `5.0`, which would write an *int* scalar and drift the field's type —
/// so a whole-number float keeps its `.0`.
fn float_repr(value: f32) -> String {
    let rendered = value.to_string();
    if rendered.contains('.') {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D18: bare olink names are auto-wrapped and always quoted; full
    /// `[[path|alias]]` links pass through as-is. Emission is
    /// [`render_field`] — [`Display`] is the human form.
    #[test]
    fn render_field_wraps_olinks() {
        assert_eq!(
            render_field("repo", &Field::Olink("repo-name".into()), "\n"),
            "repo: \"[[repo-name]]\""
        );
        assert_eq!(
            render_field("repo", &Field::Olink("[[path/Ada|Ada]]".into()), "\n"),
            "repo: \"[[path/Ada|Ada]]\""
        );
    }

    /// YAML emission escapes YAML-special characters: backslash first,
    /// then the quote — the order matters and the test pins it.
    #[test]
    fn render_field_escapes_backslash_then_quote() {
        assert_eq!(
            render_field("x", &Field::Str("a\"b\\c".into()), "\n"),
            "x: \"a\\\"b\\\\c\""
        );
    }

    /// Lists emit block style using the note's own line ending; an empty
    /// list degrades to flow `[]` (a bare `name:` would be YAML null).
    #[test]
    fn render_field_list_is_block_style() {
        assert_eq!(
            render_field(
                "tags",
                &Field::List(vec![Field::Str("a".into()), Field::Str("b c".into())]),
                "\n"
            ),
            "tags:\n  - \"a\"\n  - \"b c\""
        );
        assert_eq!(render_field("tags", &Field::List(vec![]), "\n"), "tags: []");
    }

    /// [`Display`] is the human form: bare values, comma-joined lists —
    /// deliberately not the YAML shapes [`render_field`] emits.
    #[test]
    fn display_is_human_not_yaml() {
        assert_eq!(Field::Str("a b".into()).to_string(), "a b");
        assert_eq!(Field::Bool(true).to_string(), "true");
        assert_eq!(
            Field::List(vec![Field::Str("a".into()), Field::Str("b".into())]).to_string(),
            "a, b"
        );
    }

    /// `parse` coerces scalars and splits comma lists — the input inverse
    /// of [`Display`]'s human form.
    #[test]
    fn parse_coerces_scalars_and_lists() {
        assert_eq!(
            Field::parse(&FieldType::Str, "hello").unwrap(),
            Field::Str("hello".into())
        );
        assert_eq!(Field::parse(&FieldType::Int, "5").unwrap(), Field::Int(5));
        assert_eq!(
            Field::parse(&FieldType::Bool, "true").unwrap(),
            Field::Bool(true)
        );
        assert_eq!(
            Field::parse(
                &FieldType::List(Box::new(FieldType::Str)),
                "this, is, a, tag"
            )
            .unwrap(),
            Field::List(vec![
                Field::Str("this".into()),
                Field::Str("is".into()),
                Field::Str("a".into()),
                Field::Str("tag".into())
            ])
        );
        assert_eq!(
            Field::parse(&FieldType::List(Box::new(FieldType::Int)), "1, 2").unwrap(),
            Field::List(vec![Field::Int(1), Field::Int(2)])
        );
    }

    #[test]
    fn parse_rejects_bad_values() {
        let err = Field::parse(&FieldType::Int, "abc").unwrap_err();
        assert!(err.to_string().contains("not a valid int"), "got: {err}");
        // YAML 1.2 only: `yes` is a string, never a bool — same stance the
        // reader takes on `done: yes`.
        assert!(Field::parse(&FieldType::Bool, "yes").is_err());
        // `inf`/`NaN` have no clean scalar in the hand-rolled emitter.
        assert!(Field::parse(&FieldType::Float, "inf").is_err());
        assert!(Field::parse(&FieldType::List(Box::new(FieldType::Int)), "1, x").is_err());
    }

    /// Empty elements are dropped after trimming, so `"a,,b,"` and `""`
    /// coerce cleanly (D16: an empty value empties the field).
    #[test]
    fn parse_drops_empty_list_elements() {
        assert_eq!(
            Field::parse(&FieldType::List(Box::new(FieldType::Str)), "a,,b,").unwrap(),
            Field::List(vec![Field::Str("a".into()), Field::Str("b".into())])
        );
        assert_eq!(
            Field::parse(&FieldType::List(Box::new(FieldType::Str)), "").unwrap(),
            Field::List(vec![])
        );
    }

    /// `list<list<T>>` has no input syntax — comma-split can't nest.
    #[test]
    fn parse_refuses_nested_lists() {
        let nested = FieldType::List(Box::new(FieldType::List(Box::new(FieldType::Int))));
        let err = Field::parse(&nested, "1, 2").unwrap_err();
        assert!(err.to_string().contains("nested lists"), "got: {err}");
    }

    /// A whole-number float keeps its `.0` in both forms — otherwise the
    /// note would store an *int* scalar and drift the field's type.
    #[test]
    fn float_keeps_decimal_point() {
        assert_eq!(Field::Float(5.0).to_string(), "5.0");
        assert_eq!(Field::Float(5.5).to_string(), "5.5");
        assert_eq!(
            render_field("estimate", &Field::Float(5.0), "\n"),
            "estimate: 5.0"
        );
    }

    #[test]
    fn field_types_parse_from_config_words() {
        assert_eq!("string".parse::<FieldType>().unwrap(), FieldType::Str);
        assert_eq!("olink".parse::<FieldType>().unwrap(), FieldType::Olink);
        assert_eq!(
            "list<int>".parse::<FieldType>().unwrap(),
            FieldType::List(Box::new(FieldType::Int))
        );
        assert_eq!(
            "list<list<float>>".parse::<FieldType>().unwrap(),
            FieldType::List(Box::new(FieldType::List(Box::new(FieldType::Float))))
        );

        let err = "wat".parse::<FieldType>().unwrap_err();
        assert!(err.to_string().contains("not a valid field type"));
    }
}
