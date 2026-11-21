//! Fennoly's in-house file tools.
//!
//! Every tool here is blocking and CPU or process bound. The server runs them
//! on a blocking thread pool so the async runtime stays responsive.

pub mod image_tools;
pub mod pdf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("could not read or write the image: {0}")]
    Image(#[from] image::ImageError),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ghostscript failed: {0}")]
    Ghostscript(String),
    #[error("this tool is not installed on the server")]
    NotAvailable,
    #[error("unsupported file type")]
    Unsupported,
}

/// Which tools this server can run, reported to the web app.
#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    pub image: bool,
    pub pdf: bool,
}

pub fn capabilities() -> Capabilities {
    Capabilities {
        image: true,
        pdf: pdf::ghostscript_path().is_some(),
    }
}
