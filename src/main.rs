use clap::CommandFactory;
use clap::Parser;
use color_eyre::eyre::{Result, WrapErr};
use ocli::{
    cli::{Cli, Command},
    commands,
    config::Config,
    context::Context,
};
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    // Installs color-eyre's report handlers: from here on, `?`-propagated
    // errors print as colored, span-aware reports instead of debug output.
    color_eyre::install()?;

    init_tracing()?;
    let cli = Cli::parse();

    // `completion` is pure clap output — no vault, no git, no config. It
    // runs before `Context` exists so packagers (the flake's `postInstall`)
    // can generate shell completions without mocking a vault root.
    if let Command::Completion { shell } = &cli.command {
        let mut cmd = ocli::cli::Cli::command();
        clap_complete::generate(*shell, &mut cmd, env!("CARGO_BIN_NAME"), &mut std::io::stdout());
        return Ok(());
    }

    let config = Config::load(cli.vault.clone())?;
    let context = Context::new(config, cli, &std::env::current_dir()?)?;
    tracing::debug!("ocli initialized");

    match &context.cli().command {
        Command::List { .. } => {
            let rows = commands::list::run(&context)?;
            for row in rows {
                println!("{row}");
            }
        }
        Command::New { .. } => {
            let path = commands::new::run(&context)?;
            println!("{}", path.display());
        }
        Command::Open => commands::open::run(&context)?,
        Command::FrontMatter { .. } => commands::fm::run(&context)?,
        Command::Status { .. } => commands::status::run(&context)?,
        Command::Section { .. } => {
            let section = commands::section::run(&context)?;
            section.inspect(|section| println!("{section}"));
        }
        // Handled before `Context` (needs no environment); kept only to
        // satisfy the exhaustive match.
        Command::Completion { .. } => unreachable!("completion dispatched before context"),
    };

    Ok(())
}

/// Diagnostics channel (D26): tracing events go to stderr only.
/// RUST_LOG unset → `ocli=warn`; a set-but-malformed RUST_LOG is an error
/// (fail-loud), not something to silently ignore.
fn init_tracing() -> Result<()> {
    let filter = match std::env::var("RUST_LOG") {
        Ok(spec) => EnvFilter::builder()
            .parse(&spec)
            .wrap_err_with(|| format!("invalid RUST_LOG: {spec:?}"))?,
        Err(std::env::VarError::NotPresent) => EnvFilter::new("ocli=warn"),
        Err(err) => return Err(color_eyre::eyre::eyre!("invalid RUST_LOG: {err}")),
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    Ok(())
}
