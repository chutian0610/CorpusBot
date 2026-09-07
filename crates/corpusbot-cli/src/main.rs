use clap::{Parser, Subcommand};
use corpusbot_core::{Template, VERSION};

#[derive(Debug, Parser)]
#[command(
    name = "corpusbot",
    version = VERSION,
    about = "Local knowledge-base engine"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize a new workspace.
    Init {
        #[arg(long)]
        root: std::path::PathBuf,
        #[arg(long, default_value = "generic")]
        template: String,
    },
    /// Show engine status.
    Status {
        #[arg(long)]
        root: std::path::PathBuf,
    },
}

fn parse_template(value: &str) -> anyhow::Result<Template> {
    match value {
        "generic" => Ok(Template::Generic),
        "research" => Ok(Template::Research),
        other => Err(anyhow::anyhow!("unknown template: {other}")),
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init { root, template } => {
            parse_template(&template)?;
            println!("workspace root: {}", root.display());
        }
        Command::Status { root } => {
            println!("workspace root: {}", root.display());
        }
    }
    Ok(())
}
