//! What can go wrong while writing output.

use std::fmt;

use angzarr_embedded_diagnostic::{Arg, Diagnostic};

use crate::failure::Failure;

/// A result that could not be written in the selected format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FmtError {
    /// The result did not serialize (a map with non-string keys, a failing
    /// `Serialize` impl).
    Serialize { detail: String },
    /// The output stream refused the bytes.
    Write { detail: String },
    /// A message catalog did not parse.
    CatalogParse { detail: String },
}

impl FmtError {
    pub(crate) fn write(e: std::io::Error) -> FmtError {
        FmtError::Write {
            detail: e.to_string(),
        }
    }
}

impl fmt::Display for FmtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FmtError::Serialize { detail } => {
                write!(f, "the result could not be serialized: {detail}")
            }
            FmtError::Write { detail } => write!(f, "the output could not be written: {detail}"),
            FmtError::CatalogParse { detail } => {
                write!(f, "a message catalog has a syntax error: {detail}")
            }
        }
    }
}

impl std::error::Error for FmtError {}

impl Diagnostic for FmtError {
    fn code(&self) -> &'static str {
        match self {
            FmtError::Serialize { .. } => "angzarr-fmt-serialize",
            FmtError::Write { .. } => "angzarr-fmt-write",
            FmtError::CatalogParse { .. } => "angzarr-fmt-catalog-parse",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        match self {
            FmtError::Serialize { detail }
            | FmtError::Write { detail }
            | FmtError::CatalogParse { detail } => vec![("detail", detail.as_str().into())],
        }
    }
}

impl Failure for FmtError {}

/// One of every diagnostic this crate raises, for the catalog check.
pub fn diagnostic_samples() -> Vec<Box<dyn Diagnostic>> {
    let s = || "x".to_string();
    vec![
        Box::new(FmtError::Serialize { detail: s() }),
        Box::new(FmtError::Write { detail: s() }),
        Box::new(FmtError::CatalogParse { detail: s() }),
        Box::new(crate::format::UnknownFormat { name: s() }),
    ]
}
