use std::{convert::Infallible, fmt::Display, str::FromStr};

use serde::Deserialize;
use thiserror::Error;

/// The status vocabulary (PLAN: `Backlog`, `In Progress`, `In Review`,
/// `Blocked`, `Complete`), plus [`Status::Unknown`] for anything the vault
/// holds that ocli doesn't recognize.
///
/// `Unknown` keeps the *read* path faithful: a hand-written note with
/// `status: Doing` loads and lists (showing "Doing") instead of failing the
/// note. The *input* paths are strict — [`parse_strict`] (CLI) and Phase 6's
/// writes refuse `Unknown`.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Backlog,
    InProgress,
    InReview,
    Blocked,
    Complete,
    /// A status word outside the vocabulary, preserved verbatim.
    Unknown(String),
}

impl Status {
    pub fn is_unknown(&self) -> bool {
        matches!(self, Status::Unknown(_))
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Backlog => write!(f, "Backlog"),
            Status::InProgress => write!(f, "In Progress"),
            Status::InReview => write!(f, "In Review"),
            Status::Blocked => write!(f, "Blocked"),
            Status::Complete => write!(f, "Complete"),
            Status::Unknown(s) => write!(f, "{s}"),
        }
    }
}

/// Infallible on purpose: every string maps to a `Status` — canonical
/// spellings to their variant, anything else to [`Status::Unknown`]. The
/// strict boundary is [`parse_strict`], where unknowns are refused.
impl FromStr for Status {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim() {
            "Backlog" => Status::Backlog,
            "In Progress" | "InProgress" | "progress" | "in-progress" => Status::InProgress,
            "In Review" | "InReview" | "Review" | "review" => Status::InReview,
            "Blocked" | "blocked" | "Block" | "block" => Status::Blocked,
            "Complete" | "complete" | "Done" | "done" | "Completed" | "completed" => {
                Status::Complete
            }
            other => Status::Unknown(other.to_string()),
        })
    }
}

/// A `--status` value outside the vocabulary: the CLI refuses it at argument
/// parsing time (exit 2), unlike the read path which keeps it as
/// [`Status::Unknown`].
#[derive(Debug, Error)]
#[error("unknown status {0:?}")]
pub struct UnknownStatusError(String);

/// Parses a CLI `--status`, rejecting anything that isn't a canonical status.
pub fn parse_strict(s: &str) -> Result<Status, UnknownStatusError> {
    let status: Status = s.parse().expect("Status::from_str is infallible");
    if status.is_unknown() {
        Err(UnknownStatusError(s.to_string()))
    } else {
        Ok(status)
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(s.parse().expect("Status::from_str is infallible"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_display_forms() {
        assert_eq!("Backlog".parse::<Status>().unwrap(), Status::Backlog);
        assert_eq!("In Progress".parse::<Status>().unwrap(), Status::InProgress);
        assert_eq!("In Review".parse::<Status>().unwrap(), Status::InReview);
        assert_eq!("Blocked".parse::<Status>().unwrap(), Status::Blocked);
        assert_eq!("Complete".parse::<Status>().unwrap(), Status::Complete);
    }

    #[test]
    fn aliases_canonicalize() {
        assert_eq!("done".parse::<Status>().unwrap(), Status::Complete);
        assert_eq!("in-progress".parse::<Status>().unwrap(), Status::InProgress);
    }

    #[test]
    fn unknown_word_becomes_unknown() {
        let status: Status = "Doing".parse().unwrap();
        assert_eq!(status, Status::Unknown("Doing".to_string()));
        assert!(status.is_unknown());
        assert_eq!(status.to_string(), "Doing");
    }

    #[test]
    fn parse_strict_accepts_canonical_rejects_unknown() {
        assert_eq!(parse_strict("In Progress").unwrap(), Status::InProgress);
        assert!(parse_strict("in progress").is_err());
        assert!(parse_strict("Doing").is_err());
    }

    #[test]
    fn deserializes_status_from_yaml() {
        assert_eq!(
            yaml_serde::from_str::<Status>("\"In Progress\"").unwrap(),
            Status::InProgress
        );
        assert_eq!(
            yaml_serde::from_str::<Status>("\"Doing\"").unwrap(),
            Status::Unknown("Doing".to_string())
        );
    }
}
