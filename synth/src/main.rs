use anyhow::Result;
use structopt::StructOpt;
use synth::cli::Args;
use synth::cli::Cli;

fn version() -> String {
    let current_version = synth::version::version();
    let version_update_info = synth::version::version_update_info()
        .map(|(info, _)| info)
        .unwrap_or_default()
        .map(|info| format!("\n{}", info))
        .unwrap_or_default();
    format!("{}{}", current_version, version_update_info)
}

fn setup_args() -> Args {
    let version = version();

    let mut app = Args::clap();
    app = app.version(version.as_str());

    Args::from_clap(&app.get_matches())
}

#[async_std::main]
async fn main() -> Result<()> {

    let args = setup_args();
    let cli = Cli::new()?;

    #[cfg(feature = "telemetry")]
    synth::cli::telemetry::with_telemetry(args, |args| cli.run(args)).await?;

    #[cfg(not(feature = "telemetry"))]
    cli.run(args).await?;

    // Result ignored as this should fail silently
    let _ = synth::version::notify_new_version();

    Ok(())
}
