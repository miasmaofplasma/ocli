use crate::{cli::Cli, config::Config};

pub struct Context {
    config: Config,
    cli: Cli,
}

impl Context {
    pub fn new(config: Config, cli: Cli) -> Self {
        Self { config, cli }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn cli(&self) -> &Cli {
        &self.cli
    }
}
