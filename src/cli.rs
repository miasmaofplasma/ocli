use std::path::PathBuf;

use crate::status::{Status, parse_strict};
use clap::{Parser, Subcommand};

#[derive(Debug, Subcommand)]
pub enum Command {
    List {
        #[arg(long, default_value_t = false)]
        all_repos: bool,
        #[arg(long, value_parser = parse_strict)]
        status: Option<Status>,
    },
    New {
        /// Explicit ticket key; overrides the branch-derived id.
        /// A case-insensitive mismatch with the branch warns and proceeds.
        key: Option<String>,
        #[arg(long, short)]
        description: Option<String>,
        /// Override a template value, e.g. --set FeatureType=XX; repeatable.
        #[arg(long, value_name = "NAME=VALUE")]
        set: Vec<String>,
        /// Override the repo olink fill; default is the origin's name.
        #[arg(long)]
        repo: Option<String>,
    },
    Open,
    #[command(visible_alias = "fm")]
    FrontMatter {
        key: String,
        value: String,
    },
    Status {
        #[arg(value_parser = parse_strict)]
        value: Status,
    },
}

#[derive(Debug, Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
    #[arg(long, short, global = true)]
    pub vault: Option<PathBuf>,
}
