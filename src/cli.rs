use crate::detect::ProjectType;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "forge")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    Init {
        #[arg(long = "type")]
        project_type: Option<ProjectType>,
        #[arg(long)]
        force: bool,
    },
    Run {
        blueprint_name: Option<String>,
        #[arg(long)]
        blueprint: Option<String>,
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        issue: Option<String>,
        #[arg(long)]
        round: Option<String>,
        #[arg(long)]
        pr: Option<String>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_delimiter = ',')]
        notify: Vec<String>,
        #[arg(long)]
        verbose: bool,
        #[arg(long = "var", value_parser = parse_var)]
        vars: Vec<(String, String)>,
    },
    Generate {
        #[arg(long = "type")]
        project_type: Option<ProjectType>,
        #[arg(long)]
        force: bool,
    },
    Status,
    List,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

fn parse_var(input: &str) -> Result<(String, String), String> {
    let (key, value) = input
        .split_once('=')
        .ok_or_else(|| format!("invalid variable override `{input}`; expected key=value"))?;
    if key.is_empty() {
        return Err(format!(
            "invalid variable override `{input}`; key must not be empty"
        ));
    }
    Ok((key.to_string(), value.to_string()))
}
