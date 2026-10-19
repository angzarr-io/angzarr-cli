//! What can go wrong while writing output.

use std::fmt;

use annals_diagnostic::{Arg, Diagnostic, Report};

use crate::failure::Failure;

/// A result that could not be written in the selected format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FmtError {
    /// The result did not serialize (a map with non-string keys, a failing
    /// `Serialize` impl).
    Serialize { detail: String },
    /// TOML needs a table at the top level.
    TomlNeedsTable { found: &'static str },
    /// TOML has no null, and a list cannot leave an item out.
    TomlNull { path: String },
    /// The TOML encoder refused the value.
    TomlEncode { detail: String },
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
            FmtError::TomlNeedsTable { found } => {
                write!(
                    f,
                    "TOML output needs a table at the top level, not a {found}"
                )
            }
            FmtError::TomlNull { path } => {
                write!(f, "TOML has no null: the value at {path} is empty")
            }
            FmtError::TomlEncode { detail } => {
                write!(f, "the result could not be written as TOML: {detail}")
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
            FmtError::TomlNeedsTable { .. } => "angzarr-fmt-toml-needs-table",
            FmtError::TomlNull { .. } => "angzarr-fmt-toml-null",
            FmtError::TomlEncode { .. } => "angzarr-fmt-toml-encode",
            FmtError::Write { .. } => "angzarr-fmt-write",
            FmtError::CatalogParse { .. } => "angzarr-fmt-catalog-parse",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        match self {
            FmtError::Serialize { detail }
            | FmtError::TomlEncode { detail }
            | FmtError::Write { detail }
            | FmtError::CatalogParse { detail } => vec![("detail", detail.as_str().into())],
            FmtError::TomlNeedsTable { found } => vec![("found", (*found).into())],
            FmtError::TomlNull { path } => vec![("path", path.as_str().into())],
        }
    }
}

impl Failure for FmtError {
    fn remedy(&self) -> Option<Report> {
        match self {
            FmtError::TomlNeedsTable { .. } | FmtError::TomlNull { .. } => Some(Report::new(
                "angzarr-fmt-use-json-or-yaml",
                "use --format json or --format yaml for this output",
            )),
            _ => None,
        }
    }
}

/// One of every diagnostic this crate raises, for the catalog check.
pub fn diagnostic_samples() -> Vec<Box<dyn Diagnostic>> {
    let s = || "x".to_string();
    let mut samples: Vec<Box<dyn Diagnostic>> = vec![
        Box::new(FmtError::Serialize { detail: s() }),
        Box::new(FmtError::TomlNeedsTable { found: "list" }),
        Box::new(FmtError::TomlNull { path: s() }),
        Box::new(FmtError::TomlEncode { detail: s() }),
        Box::new(FmtError::Write { detail: s() }),
        Box::new(FmtError::CatalogParse { detail: s() }),
        Box::new(crate::format::UnknownFormat { name: s() }),
    ];
    samples.extend(
        FmtError::TomlNeedsTable { found: "list" }
            .remedy()
            .map(|r| Box::new(r) as Box<dyn Diagnostic>),
    );
    samples
}
