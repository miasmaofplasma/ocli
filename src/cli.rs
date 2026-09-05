use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Subcommand)]
pub enum Command {
    List {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        status: Option<String>,
    },
    New {
        #[arg(long, short)]
        description: Option<String>,
    },
    Open,
}

#[derive(Debug, Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
    #[arg(long, short, global = true)]
    pub vault: Option<PathBuf>,
}
