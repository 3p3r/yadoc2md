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

pub fn run_inner(args: ParseArgs) -> Result<(), AppError> {
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
        write_stdout(&markdown)?;
    }

    Ok(())
}

fn write_stdout(markdown: &str) -> Result<(), AppError> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(markdown.as_bytes())
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    stdout
        .flush()
        .map_err(|e| AppError::InvalidInput(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn run_inner_writes_stdout_for_txt() {
        let args = ParseArgs {
            config: ConvertConfig::default(),
            file: fixture("sample.txt"),
            output: None,
        };
        run_inner(args).unwrap();
    }

    #[test]
    fn run_inner_writes_output_file() {
        let out = std::env::temp_dir().join(format!("yadoc2md-parse-{}.md", std::process::id()));
        let args = ParseArgs {
            config: ConvertConfig::default(),
            file: fixture("sample.md"),
            output: Some(out.clone()),
        };
        run_inner(args).unwrap();
        let written = fs::read_to_string(&out).unwrap();
        assert!(written.contains('#'));
        let _ = fs::remove_file(out);
    }

    #[test]
    fn run_inner_fails_for_missing_file() {
        let args = ParseArgs {
            config: ConvertConfig::default(),
            file: PathBuf::from("/nonexistent/yadoc2md-file.txt"),
            output: None,
        };
        assert!(run_inner(args).is_err());
    }

    #[test]
    fn run_inner_fails_when_file_exceeds_limit() {
        let args = ParseArgs {
            config: ConvertConfig {
                max_input_bytes: 1,
                ..Default::default()
            },
            file: fixture("sample.txt"),
            output: None,
        };
        assert!(matches!(
            run_inner(args),
            Err(AppError::InputTooLarge { .. })
        ));
    }

    #[test]
    fn run_inner_fails_for_unsupported_format() {
        let args = ParseArgs {
            config: ConvertConfig::default(),
            file: fixture("sample.css"),
            output: None,
        };
        assert!(run_inner(args).is_err());
    }

    #[test]
    fn run_returns_exit_code() {
        let ok = run(ParseArgs {
            config: ConvertConfig::default(),
            file: fixture("sample.txt"),
            output: None,
        });
        assert_eq!(ok, 0);

        let bad = run(ParseArgs {
            config: ConvertConfig::default(),
            file: PathBuf::from("/no/such/file.txt"),
            output: None,
        });
        assert_eq!(bad, 1);
    }
}
