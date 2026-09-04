use color_eyre::eyre::WrapErr;
use tracing_subscriber::EnvFilter;

fn main() -> color_eyre::eyre::Result<()> {
    // Installs color-eyre's report handlers: from here on, `?`-propagated
    // errors print as colored, span-aware reports instead of debug output.
    color_eyre::install()?;

    init_tracing()?;

    tracing::debug!("ocli initialized");
    Ok(())
}

/// Diagnostics channel (D26): tracing events go to stderr only.
/// RUST_LOG unset → `ocli=warn`; a set-but-malformed RUST_LOG is an error
/// (fail-loud), not something to silently ignore.
fn init_tracing() -> color_eyre::eyre::Result<()> {
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
