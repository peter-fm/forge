use clap::Parser;
use forge::cli::Cli;

fn main() {
    if let Err(error) = forge::commands::dispatch(Cli::parse()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
