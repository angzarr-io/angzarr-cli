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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, EN_US};

    /// The message ids of this crate's catalog.
    fn messages() -> Vec<&'static str> {
        let mut ids: Vec<&str> = EN_US
            .lines()
            .filter_map(|line| line.split_once(" = ").map(|(id, _)| id))
            .filter(|id| id.starts_with("angzarr-fmt-"))
            .collect();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn every_catalog_message_is_the_code_of_a_sample() {
        let mut codes: Vec<&str> = diagnostic_samples().iter().map(|d| d.code()).collect();
        codes.sort_unstable();
        assert_eq!(codes, messages());
    }

    #[test]
    fn every_errors_english_is_its_catalog_message() {
        let mut catalog = Catalog::en_us();
        catalog.add(EN_US).unwrap();
        for sample in diagnostic_samples() {
            let rendered: String = catalog
                .render(&*sample)
                .chars()
                .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
                .collect();
            assert_eq!(sample.to_string(), rendered, "{}", sample.code());
        }
    }
}
