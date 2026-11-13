//! Input limits set for S1. Reject oversize text and non-https public URIs.

use crate::error::Error;

/// Maximum source or evidence URL length, in bytes.
pub const MAX_URL_LEN: usize = 2048;
/// Maximum exact-quote length, in bytes.
pub const MAX_QUOTE_LEN: usize = 8192;
/// Maximum length for long text fields (question, resolution rule, basis, notices).
pub const MAX_TEXT_LEN: usize = 16_384;
/// Maximum length for ids and short names.
pub const MAX_ID_LEN: usize = 256;
/// Maximum claim-file body size, in bytes, enforced by the store.
pub const MAX_FILE_BYTES: usize = 65_536;

pub fn check_len(field: &'static str, value: &str, max: usize) -> Result<(), Error> {
    let got = value.len();
    if got > max {
        Err(Error::InputTooLong { field, max, got })
    } else {
        Ok(())
    }
}

/// Public source and evidence URIs must be https and within [`MAX_URL_LEN`].
pub fn check_https_uri(field: &'static str, uri: &str) -> Result<(), Error> {
    check_len(field, uri, MAX_URL_LEN)?;
    let lower = uri.to_ascii_lowercase();
    if !lower.starts_with("https://")
        || uri[8..].is_empty()
        || uri[8..].contains(char::is_whitespace)
    {
        return Err(Error::NonHttpsEvidenceUri {
            uri: uri.to_string(),
        });
    }
    Ok(())
}

pub fn check_finite(field: &'static str, value: f64) -> Result<(), Error> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(Error::InvalidNumeric {
            field,
            reason: "must be a finite number".into(),
        })
    }
}

pub fn check_probability(value: f64) -> Result<(), Error> {
    check_finite("probability", value)?;
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(Error::InvalidProbability {
            reason: format!("probability must be in [0, 1], got {value}"),
        })
    }
}
