use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use clap::Args;

use crate::config::ConvertConfig;
use crate::convert::{convert, AppError};

#[derive(Debug, Args)]
pub struct ParseArgs {
    #[command(flatten)]
    pub config: ConvertConfig,

    /// Input document path (format inferred from extension).
    pub file: PathBuf,

    /// Write markdown to this file instead of stdout.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

pub fn run(args: ParseArgs) -> i32 {
    match run_inner(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn run_inner(args: ParseArgs) -> Result<(), AppError> {
    let meta = fs::metadata(&args.file).map_err(|e| AppError::InvalidInput(e.to_string()))?;
    let len = meta.len() as usize;
    if len > args.config.max_input_bytes {
        return Err(AppError::InputTooLarge {
            max: args.config.max_input_bytes,
            got: len,
        });
    }

    let data = fs::read(&args.file).map_err(|e| AppError::InvalidInput(e.to_string()))?;
    let filename = args
        .file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("input");
    let markdown = convert(filename, &data, &args.config)?;

    if let Some(path) = args.output {
        fs::write(&path, &markdown).map_err(|e| AppError::InvalidInput(e.to_string()))?;
    } else {
        let mut stdout = io::stdout().lock();
        stdout
            .write_all(markdown.as_bytes())
            .map_err(|e| AppError::InvalidInput(e.to_string()))?;
        stdout
            .flush()
            .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    }

    Ok(())
}
