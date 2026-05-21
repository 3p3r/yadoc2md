use anytomd::ConvertError;
use thiserror::Error;
use unpdf::Unpdf;

use crate::config::{extension_from_filename, ConvertConfig};

#[derive(Debug, Error)]
pub enum AppError {
    #[error("input exceeds --max-input-size ({max} bytes, got {got})")]
    InputTooLarge { max: usize, got: usize },

    #[error("{0}")]
    InvalidInput(String),

    #[error("conversion failed: {0}")]
    Anytomd(#[from] ConvertError),

    #[error("PDF conversion failed: {0}")]
    Unpdf(#[from] unpdf::Error),
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
