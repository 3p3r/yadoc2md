use std::path::Path;

use anytomd::ConversionOptions;
use clap::Args;

pub const DEFAULT_MAX_INPUT: &str = "100MB";
pub const DEFAULT_MAX_ZIP: &str = "500MB";
pub const DEFAULT_MAX_IMAGE: &str = "50MB";

#[derive(Debug, Clone, Args)]
pub struct ConvertConfig {
    /// Maximum input file size (e.g. 100MB, 1GB).
    #[arg(long = "max-input-size", default_value = DEFAULT_MAX_INPUT, value_parser = parse_byte_size)]
    pub max_input_bytes: usize,

    /// Maximum uncompressed ZIP size for Office archives (ZIP bomb guard).
    #[arg(long = "max-zip-size", default_value = DEFAULT_MAX_ZIP, value_parser = parse_byte_size)]
    pub max_uncompressed_zip_bytes: usize,

    /// Maximum total extracted image bytes.
    #[arg(long = "max-image-bytes", default_value = DEFAULT_MAX_IMAGE, value_parser = parse_byte_size)]
    pub max_total_image_bytes: usize,

    /// Treat recoverable conversion issues as hard errors.
    #[arg(long)]
    pub strict: bool,

    /// Password for encrypted PDFs.
    #[arg(long = "pdf-password")]
    pub pdf_password: Option<String>,
}

impl Default for ConvertConfig {
    fn default() -> Self {
        Self {
            max_input_bytes: parse_byte_size(DEFAULT_MAX_INPUT).expect("valid default"),
            max_uncompressed_zip_bytes: parse_byte_size(DEFAULT_MAX_ZIP).expect("valid default"),
            max_total_image_bytes: parse_byte_size(DEFAULT_MAX_IMAGE).expect("valid default"),
            strict: false,
            pdf_password: None,
        }
    }
}

impl ConvertConfig {
    pub fn anytomd_options(&self) -> ConversionOptions {
        ConversionOptions {
            extract_images: false,
            max_total_image_bytes: self.max_total_image_bytes,
            strict: self.strict,
            max_input_bytes: self.max_input_bytes,
            max_uncompressed_zip_bytes: self.max_uncompressed_zip_bytes,
            image_describer: None,
        }
    }
}

pub fn extension_from_filename(filename: &str) -> Result<String, String> {
    let path = Path::new(filename);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .ok_or_else(|| format!("no file extension in {filename:?}"))?;
    Ok(ext.to_ascii_lowercase())
}

pub fn parse_byte_size(s: &str) -> Result<usize, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty size".into());
    }

    let (num_part, unit_part) = if let Some(idx) = s.find(|c: char| c.is_alphabetic()) {
        (&s[..idx], s[idx..].trim())
    } else {
        (s, "B")
    };

    let value: f64 = num_part
        .trim()
        .parse()
        .map_err(|_| format!("invalid size number: {num_part:?}"))?;

    if !value.is_finite() || value < 0.0 {
        return Err(format!("invalid size: {s}"));
    }

    let multiplier: u64 = match unit_part.to_ascii_uppercase().as_str() {
        "B" | "" => 1,
        "KB" | "KIB" | "K" => 1024,
        "MB" | "MIB" | "M" => 1024 * 1024,
        "GB" | "GIB" | "G" => 1024 * 1024 * 1024,
        other => return Err(format!("unknown size unit: {other}")),
    };

    let bytes = value * multiplier as f64;
    if bytes > usize::MAX as f64 {
        return Err(format!("size too large: {s}"));
    }
    Ok(bytes as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_byte_size("100MB").unwrap(), 100 * 1024 * 1024);
        assert_eq!(parse_byte_size("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_byte_size("512").unwrap(), 512);
        assert_eq!(parse_byte_size("1KB").unwrap(), 1024);
    }

    #[test]
    fn parse_byte_size_rejects_empty_and_invalid() {
        assert!(parse_byte_size("").is_err());
        assert!(parse_byte_size("abc").is_err());
        assert!(parse_byte_size("-1MB").is_err());
        assert!(parse_byte_size("1PB").is_err());
    }

    #[test]
    fn extracts_extension() {
        assert_eq!(extension_from_filename("doc.PDF").unwrap(), "pdf");
        assert_eq!(extension_from_filename("a.csv").unwrap(), "csv");
        assert_eq!(extension_from_filename("path/to/file.HTML").unwrap(), "html");
    }

    #[test]
    fn extension_errors_without_dot() {
        assert!(extension_from_filename("nodot").is_err());
        assert!(extension_from_filename("file.").is_err());
    }

    #[test]
    fn default_config_matches_library_limits() {
        let cfg = ConvertConfig::default();
        assert_eq!(cfg.max_input_bytes, 100 * 1024 * 1024);
        assert!(!cfg.strict);
        assert!(cfg.pdf_password.is_none());
    }

    #[test]
    fn anytomd_options_disables_image_extraction() {
        let opts = ConvertConfig::default().anytomd_options();
        assert!(!opts.extract_images);
        assert!(opts.image_describer.is_none());
    }
}
