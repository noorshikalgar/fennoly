//! PDF compression through Ghostscript (AGPL-3.0, run as a separate process).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ToolError;

/// Finds the Ghostscript binary: `FENNOLY_GHOSTSCRIPT`, then `gs`,
/// `gswin64c` and `gswin32c` on the PATH.
pub fn ghostscript_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FENNOLY_GHOSTSCRIPT") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let path = std::env::var_os("PATH")?;
    let names: &[&str] = if cfg!(windows) {
        &["gswin64c.exe", "gswin32c.exe", "gs.exe"]
    } else {
        &["gs"]
    };
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

/// Compression settings derived from a 0–100 strength slider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfSettings {
    pub image_dpi: u32,
    pub preset: &'static str,
}

impl PdfSettings {
    /// 0 = lightest touch (print quality), 100 = smallest file.
    pub fn from_strength(strength: u8) -> Self {
        let s = strength.min(100) as u32;
        let image_dpi = (300 - s * 240 / 100).max(60);
        let preset = match s {
            70.. => "/screen",
            35..=69 => "/ebook",
            _ => "/printer",
        };
        PdfSettings { image_dpi, preset }
    }
}

/// Compresses `input` into `output`. Blocking; may take a few seconds.
pub fn compress(input: &Path, output: &Path, strength: u8) -> Result<(), ToolError> {
    let gs = ghostscript_path().ok_or(ToolError::NotAvailable)?;
    let s = PdfSettings::from_strength(strength);
    let dpi = s.image_dpi.to_string();
    let result = Command::new(gs)
        .args([
            "-sDEVICE=pdfwrite",
            "-dCompatibilityLevel=1.5",
            "-dNOPAUSE",
            "-dBATCH",
            "-dQUIET",
            "-dSAFER",
            "-dDetectDuplicateImages=true",
            "-dCompressFonts=true",
            "-dDownsampleColorImages=true",
            "-dDownsampleGrayImages=true",
            "-dDownsampleMonoImages=true",
            "-dColorImageDownsampleType=/Bicubic",
            "-dGrayImageDownsampleType=/Bicubic",
        ])
        .arg(format!("-dPDFSETTINGS={}", s.preset))
        .arg(format!("-dColorImageResolution={dpi}"))
        .arg(format!("-dGrayImageResolution={dpi}"))
        .arg(format!("-dMonoImageResolution={}", (s.image_dpi * 2).min(600)))
        .arg(format!("-sOutputFile={}", output.display()))
        .arg(input)
        .output()?;
    if !result.status.success() {
        let msg = String::from_utf8_lossy(&result.stderr);
        return Err(ToolError::Ghostscript(msg.chars().take(400).collect()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_maps_to_settings() {
        assert_eq!(PdfSettings::from_strength(0), PdfSettings { image_dpi: 300, preset: "/printer" });
        assert_eq!(PdfSettings::from_strength(50).preset, "/ebook");
        assert_eq!(PdfSettings::from_strength(100), PdfSettings { image_dpi: 60, preset: "/screen" });
        assert!(PdfSettings::from_strength(80).image_dpi < PdfSettings::from_strength(20).image_dpi);
    }
}
