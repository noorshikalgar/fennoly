//! The parts of the [tus 1.0 protocol](https://tus.io/protocols/resumable-upload)
//! that do not depend on storage: header names, `Upload-Metadata` and
//! `Upload-Concat` parsing.
//!
//! Fennoly supports the core protocol plus the `creation`, `termination` and
//! `concatenation` extensions. Concatenation is what makes uploads parallel:
//! the client splits a file into parts, uploads them at the same time as
//! "partial" uploads, then asks the server to join them with one "final" upload.

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use thiserror::Error;
use uuid::Uuid;

pub const TUS_VERSION: &str = "1.0.0";
pub const TUS_EXTENSIONS: &str = "creation,creation-with-upload,termination,concatenation";
pub const OFFSET_CONTENT_TYPE: &str = "application/offset+octet-stream";

pub mod header {
    pub const TUS_RESUMABLE: &str = "tus-resumable";
    pub const TUS_VERSION: &str = "tus-version";
    pub const TUS_EXTENSION: &str = "tus-extension";
    pub const TUS_MAX_SIZE: &str = "tus-max-size";
    pub const UPLOAD_LENGTH: &str = "upload-length";
    pub const UPLOAD_OFFSET: &str = "upload-offset";
    pub const UPLOAD_METADATA: &str = "upload-metadata";
    pub const UPLOAD_CONCAT: &str = "upload-concat";
    pub const UPLOAD_DEFER_LENGTH: &str = "upload-defer-length";
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TusError {
    #[error("invalid Upload-Metadata header")]
    BadMetadata,
    #[error("invalid Upload-Concat header")]
    BadConcat,
    #[error("invalid number in header")]
    BadNumber,
}

/// Parses `Upload-Metadata`: comma-separated `key base64value` pairs.
/// A key may appear without a value.
pub fn parse_metadata(raw: &str) -> Result<HashMap<String, String>, TusError> {
    let mut out = HashMap::new();
    for pair in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let mut parts = pair.splitn(2, ' ');
        let key = parts.next().ok_or(TusError::BadMetadata)?.trim();
        if key.is_empty() {
            return Err(TusError::BadMetadata);
        }
        let value = match parts.next().map(str::trim) {
            Some(v) if !v.is_empty() => {
                let bytes = STANDARD.decode(v).map_err(|_| TusError::BadMetadata)?;
                String::from_utf8(bytes).map_err(|_| TusError::BadMetadata)?
            }
            _ => String::new(),
        };
        out.insert(key.to_string(), value);
    }
    Ok(out)
}

/// Builds an `Upload-Metadata` header value (used by tests and clients).
pub fn encode_metadata<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    pairs
        .into_iter()
        .map(|(k, v)| format!("{k} {}", STANDARD.encode(v)))
        .collect::<Vec<_>>()
        .join(",")
}

/// The value of an `Upload-Concat` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Concat {
    /// This upload is one part of a larger file.
    Partial,
    /// This upload joins the listed partial uploads, in order.
    Final(Vec<Uuid>),
}

/// Parses `Upload-Concat`. Partial upload references in a `final` value are
/// URLs (absolute or relative); the last path segment is the upload id.
pub fn parse_concat(raw: &str) -> Result<Concat, TusError> {
    let raw = raw.trim();
    if raw == "partial" {
        return Ok(Concat::Partial);
    }
    let rest = raw.strip_prefix("final;").ok_or(TusError::BadConcat)?;
    let ids = rest
        .split_whitespace()
        .map(|url| {
            let last = url.trim_end_matches('/').rsplit('/').next().unwrap_or(url);
            Uuid::parse_str(last).map_err(|_| TusError::BadConcat)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if ids.is_empty() {
        return Err(TusError::BadConcat);
    }
    Ok(Concat::Final(ids))
}

/// Parses a non-negative integer header such as `Upload-Length`.
pub fn parse_u64(raw: &str) -> Result<u64, TusError> {
    raw.trim().parse::<u64>().map_err(|_| TusError::BadNumber)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_round_trip_with_unicode() {
        let header = encode_metadata([("filename", "आधार कार्ड.pdf"), ("parentId", "abc")]);
        let parsed = parse_metadata(&header).unwrap();
        assert_eq!(parsed["filename"], "आधार कार्ड.pdf");
        assert_eq!(parsed["parentId"], "abc");
    }

    #[test]
    fn metadata_allows_empty_values() {
        let parsed = parse_metadata("is_confidential,filename d29ybGQ=").unwrap();
        assert_eq!(parsed["is_confidential"], "");
        assert_eq!(parsed["filename"], "world");
    }

    #[test]
    fn metadata_rejects_bad_base64() {
        assert_eq!(parse_metadata("filename ***"), Err(TusError::BadMetadata));
    }

    #[test]
    fn concat_parsing() {
        assert_eq!(parse_concat("partial").unwrap(), Concat::Partial);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let header = format!("final;/api/uploads/{a} https://home.lan/api/uploads/{b}");
        assert_eq!(parse_concat(&header).unwrap(), Concat::Final(vec![a, b]));
        assert_eq!(parse_concat("final;"), Err(TusError::BadConcat));
        assert_eq!(parse_concat("whole"), Err(TusError::BadConcat));
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_u64(" 1024 ").unwrap(), 1024);
        assert_eq!(parse_u64("-1"), Err(TusError::BadNumber));
    }
}
