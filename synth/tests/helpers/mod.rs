use anyhow::Result;
use synth::cli::{self, GenerateCommand};

/// Helper to capture and return any output for generate on a namespace
pub async fn generate(namespace: &str) -> Result<String> {
    generate_scenario(namespace, None).await
}

pub async fn generate_scenario(namespace: &str, scenario: Option<String>) -> Result<String> {
    run(cli::Args::Generate(GenerateCommand {
        namespace: namespace.into(),
        collection: None,
        scenario,
        random: false,
        schema: None,
        seed: Some(5),
        size: 10,
        to: "json:".to_string(),
        concurrency: 3,
    }))
    .await
}

/// Helper to capture and return any output for Cli::run
async fn run(args: cli::Args) -> Result<String> {
    let mut dummy = Vec::new();

    {
        let cli = cli::Cli::new()?;
        cli.run(args, &mut dummy).await?;
    }

    let output = String::from_utf8(dummy)?;

    Ok(output)
}
