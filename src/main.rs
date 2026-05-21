mod config;
mod convert;
mod parse;
mod serve;

use clap::{Parser, Subcommand};
use parse::ParseArgs;
use serve::ServeArgs;

#[derive(Debug, Parser)]
#[command(name = "yadoc2md", about = "Document to Markdown converter")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Convert a file to markdown (stdout or -o).
    Parse(ParseArgs),
    /// Start the REST API server.
    Serve(ServeArgs),
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Parse(args) => parse::run(args),
        Command::Serve(args) => match tokio::runtime::Runtime::new() {
            Ok(rt) => match rt.block_on(serve::run(args)) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            },
            Err(e) => {
                eprintln!("failed to start runtime: {e}");
                1
            }
        },
    };
    std::process::exit(code);
}
