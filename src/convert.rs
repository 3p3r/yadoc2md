use anytomd::ConvertError;
use thiserror::Error;
use unpdf::Unpdf;

use crate::config::{extension_from_filename, ConvertConfig};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AppError {
    #[error("input exceeds --max-input-size ({max} bytes, got {got})")]
    InputTooLarge { max: usize, got: usize },

    #[error("{0}")]
    InvalidInput(String),

    #[error("conversion failed: {0}")]
    Anytomd(String),

    #[error("PDF conversion failed: {0}")]
    Unpdf(String),
}

impl From<ConvertError> for AppError {
    fn from(e: ConvertError) -> Self {
        Self::Anytomd(e.to_string())
    }
}

impl From<unpdf::Error> for AppError {
    fn from(e: unpdf::Error) -> Self {
        Self::Unpdf(e.to_string())
    }
}

pub fn convert(filename_hint: &str, data: &[u8], cfg: &ConvertConfig) -> Result<String, AppError> {
    if data.len() > cfg.max_input_bytes {
        return Err(AppError::InputTooLarge {
            max: cfg.max_input_bytes,
            got: data.len(),
        });
    }

    let ext = extension_from_filename(filename_hint).map_err(AppError::InvalidInput)?;

    match ext.as_str() {
        "pdf" => {
            let mut builder = Unpdf::new();
            if let Some(pw) = &cfg.pdf_password {
                builder = builder.with_password(pw.clone());
            }
            Ok(builder.parse_bytes(data)?.to_markdown()?)
        }
        _ => {
            let options = cfg.anytomd_options();
            Ok(anytomd::convert_bytes(data, &ext, &options)?.markdown)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name)
    }

    #[test]
    fn rejects_input_over_limit() {
        let cfg = ConvertConfig {
            max_input_bytes: 2,
            ..Default::default()
        };
        let err = convert("f.txt", b"abc", &cfg).unwrap_err();
        assert_eq!(
            err,
            AppError::InputTooLarge {
                max: 2,
                got: 3
            }
        );
    }

    #[test]
    fn rejects_missing_extension() {
        let err = convert("README", b"x", &ConvertConfig::default()).unwrap_err();
        assert_eq!(
            err,
            AppError::InvalidInput("no file extension in \"README\"".to_string())
        );
    }

    #[test]
    fn rejects_unsupported_format() {
        let data = std::fs::read(fixture("sample.css")).unwrap();
        let err = convert("sample.css", &data, &ConvertConfig::default()).unwrap_err();
        assert!(matches!(err, AppError::Anytomd(_)));
    }

    #[test]
    fn converts_txt_fixture() {
        let data = std::fs::read(fixture("sample.txt")).unwrap();
        let md = convert("sample.txt", &data, &ConvertConfig::default()).unwrap();
        assert!(md.contains("This is a sample plain text file"));
    }

    #[test]
    fn converts_csv_fixture() {
        let data = std::fs::read(fixture("sample.json")).unwrap();
        let md = convert("sample.json", &data, &ConvertConfig::default()).unwrap();
        assert!(md.contains("```json"));
    }

    #[test]
    fn converts_pdf_fixture() {
        let data = std::fs::read(fixture("sample.pdf")).unwrap();
        let md = convert("sample.pdf", &data, &ConvertConfig::default()).unwrap();
        assert!(!md.is_empty());
    }

    #[test]
    fn maps_anytomd_error_variant() {
        let err = convert("x.css", b"a{}", &ConvertConfig::default()).unwrap_err();
        assert!(matches!(err, AppError::Anytomd(_)));
    }

    #[test]
    fn routes_pdf_by_extension() {
        let data = std::fs::read(fixture("sample.pdf")).unwrap();
        let md = convert("doc.PDF", &data, &ConvertConfig::default()).unwrap();
        assert!(!md.is_empty());
    }
}
