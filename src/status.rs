use std::{fmt::Display, str::FromStr};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Backlog,
    InProgress,
    InReview,
    Blocked,
    Complete,
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Backlog => write!(f, "Backlog"),
            Status::InProgress => write!(f, "In Progress"),
            Status::InReview => write!(f, "In Review"),
            Status::Blocked => write!(f, "Blocked"),
            Status::Complete => write!(f, "Complete"),
        }
    }
}

#[derive(Debug, Error)]
#[error("unknown status {0}")]
pub struct StatusParseError(String);

impl FromStr for Status {
    type Err = StatusParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "Backlog" => Ok(Status::Backlog),
            "In Progress" | "in progress" | "InProgress" | "progress" | "in-progress" => {
                Ok(Status::InProgress)
            }
            "In Review" | "InReview" | "Review" | "review" => Ok(Status::InReview),
            "Blocked" | "blocked" | "Block" | "block" => Ok(Status::Blocked),
            "Complete" | "complete" | "Done" | "done" | "Completed" | "completed" => {
                Ok(Status::Complete)
            }
            _ => Err(StatusParseError(s.to_string())),
        }
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_status_from_yaml() {
        assert_eq!(
            yaml_serde::from_str::<Status>("\"In Progress\"").unwrap(),
            Status::InProgress
        );
    }

    #[test]
    fn deserialize_routes_through_from_str() {
        // "done" is a FromStr alias, not a serialized variant name.
        assert_eq!(
            yaml_serde::from_str::<Status>("\"done\"").unwrap(),
            Status::Complete
        );
    }

    #[test]
    fn yaml_rejects_unknown_status() {
        assert!(yaml_serde::from_str::<Status>("\"Not A Status\"").is_err());
    }
}
