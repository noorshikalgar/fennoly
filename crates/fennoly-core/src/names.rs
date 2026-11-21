//! File and folder name rules.
//!
//! Fennoly stores files as plain files on disk, in a folder tree that mirrors
//! what users see. Names must therefore be valid on Linux, Windows and macOS,
//! and must never escape their parent folder.

use thiserror::Error;

/// Longest allowed name, in bytes (the common filesystem limit).
pub const MAX_NAME_BYTES: usize = 255;

const FORBIDDEN_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NameError {
    #[error("name is empty")]
    Empty,
    #[error("name is longer than {MAX_NAME_BYTES} bytes")]
    TooLong,
    #[error("name contains a character that is not allowed: {0:?}")]
    ForbiddenChar(char),
    #[error("name is reserved")]
    Reserved,
    #[error("name cannot end with a dot or a space")]
    BadEnding,
}

/// Checks a user-supplied name and returns the cleaned version
/// (surrounding whitespace removed).
pub fn validate_name(raw: &str) -> Result<String, NameError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(NameError::TooLong);
    }
    if name == "." || name == ".." {
        return Err(NameError::Reserved);
    }
    if let Some(c) = name
        .chars()
        .find(|c| c.is_control() || FORBIDDEN_CHARS.contains(c))
    {
        return Err(NameError::ForbiddenChar(c));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(NameError::BadEnding);
    }
    let stem = name.split('.').next().unwrap_or(name);
    if WINDOWS_RESERVED
        .iter()
        .any(|r| r.eq_ignore_ascii_case(stem.trim()))
    {
        return Err(NameError::Reserved);
    }
    Ok(name.to_string())
}

/// Splits `report.final.pdf` into (`report.final`, `.pdf`).
/// Names that start with a dot (`.env`) have no extension.
pub fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// Returns `name` if it is free, otherwise `name (1)`, `name (2)`, …,
/// keeping the extension. `taken` is checked case-insensitively because
/// Windows and macOS filesystems are case-insensitive.
pub fn unique_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let (stem, ext) = split_extension(name);
    for n in 1.. {
        let candidate = format!("{stem} ({n}){ext}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    unreachable!("an unbounded range always yields a free name")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_names_in_any_script() {
        assert_eq!(validate_name("  Aadhaar card.pdf ").unwrap(), "Aadhaar card.pdf");
        assert!(validate_name("आधार कार्ड.pdf").is_ok());
        assert!(validate_name("मराठी शाळा दाखला.jpg").is_ok());
        assert!(validate_name(".hidden").is_ok());
    }

    #[test]
    fn rejects_path_tricks() {
        assert_eq!(validate_name(".."), Err(NameError::Reserved));
        assert_eq!(validate_name("a/b"), Err(NameError::ForbiddenChar('/')));
        assert_eq!(validate_name("a\\b"), Err(NameError::ForbiddenChar('\\')));
        assert_eq!(validate_name("a\0b"), Err(NameError::ForbiddenChar('\0')));
    }

    #[test]
    fn rejects_windows_problems() {
        assert_eq!(validate_name("CON"), Err(NameError::Reserved));
        assert_eq!(validate_name("nul.txt"), Err(NameError::Reserved));
        assert_eq!(validate_name("file."), Err(NameError::BadEnding));
        assert_eq!(validate_name("a:b"), Err(NameError::ForbiddenChar(':')));
        assert!(validate_name("console.log").is_ok());
    }

    #[test]
    fn rejects_empty_and_long() {
        assert_eq!(validate_name("   "), Err(NameError::Empty));
        assert_eq!(validate_name(&"a".repeat(256)), Err(NameError::TooLong));
    }

    #[test]
    fn unique_name_keeps_extension() {
        let taken = ["scan.pdf", "scan (1).pdf"];
        let is_taken = |n: &str| taken.iter().any(|t| t.eq_ignore_ascii_case(n));
        assert_eq!(unique_name("new.pdf", is_taken), "new.pdf");
        assert_eq!(unique_name("SCAN.pdf", is_taken), "SCAN (2).pdf");
        assert_eq!(unique_name("archive.tar.gz", |n| n == "archive.tar.gz"), "archive.tar (1).gz");
        assert_eq!(unique_name("Folder", |n| n == "Folder"), "Folder (1)");
    }
}
