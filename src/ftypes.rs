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
#[derive(Debug, PartialEq)]
pub enum FieldType {
    String,
    Int,
    Float,
    Bool,
    Olink,
    List(Box<FieldType>),
}

#[derive(Debug, Error)]
#[error("{0} is not a valid field type")]
pub struct FieldTypeError(pub String);

impl FromStr for FieldType {
    type Err = FieldTypeError;

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
}

impl Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Field::Str(value) => write!(f, "\"{}\"", escape_yaml_double_quoted(value)),
            // A bare name is auto-wrapped; a full `[[path|alias]]` is
            // emitted as-is (D18). `set_field`'s validation guarantees
            // only these two shapes reach here.
            Field::Olink(value) if value.starts_with("[[") => write!(f, "\"{value}\""),
            Field::Olink(value) => write!(f, "\"[[{value}]]\""),
        }
    }
}

/// YAML double-quoted-style escaping: backslash first, then the quote.
pub(crate) fn escape_yaml_double_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D18: bare olink names are auto-wrapped and always quoted; full
    /// `[[path|alias]]` links pass through as-is.
    #[test]
    fn olink_wraps_bare_names_and_keeps_full_links() {
        assert_eq!(
            Field::Olink("repo-name".into()).to_string(),
            "\"[[repo-name]]\""
        );
        assert_eq!(
            Field::Olink("[[path/Ada|Ada]]".into()).to_string(),
            "\"[[path/Ada|Ada]]\""
        );
    }

    /// Emission escapes YAML-special characters: backslash first, then
    /// the quote — the order matters and the test pins it.
    #[test]
    fn str_escapes_backslash_then_quote() {
        assert_eq!(Field::Str("a\"b\\c".into()).to_string(), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn field_types_parse_from_config_words() {
        assert_eq!("string".parse::<FieldType>().unwrap(), FieldType::String);
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
