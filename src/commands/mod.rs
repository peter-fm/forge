use crate::cli::{Cli, Commands};
use crate::error::ForgeError;

pub mod generate;
pub mod init;
pub mod list;
pub mod run;
pub mod status;

pub fn dispatch(cli: Cli) -> Result<(), ForgeError> {
    let root = std::env::current_dir()?;
    match cli.command {
        Commands::Init {
            project_type,
            force,
        } => {
            init::init_project(
                &root,
                &init::InitOptions {
                    project_type,
                    force,
                },
            )?;
            println!("initialized .forge/");
            Ok(())
        }
        Commands::Generate {
            project_type,
            force,
        } => {
            generate::generate_project(
                &root,
                &generate::GenerateOptions {
                    project_type,
                    force,
                },
            )?;
            println!("regenerated .forge/");
            Ok(())
        }
        Commands::Run { .. } => run::run_command(&root, &cli.command),
        Commands::Status => status::print_status(&root),
        Commands::List => list::list_blueprints(&root),
    }
}
