//! What can go wrong while writing output.

use std::fmt;

use angzarr_diagnostic::{Arg, Diagnostic, code};

/// A result that could not be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FmtError {
    /// The result did not serialize (a map with non-string keys, a failing
    /// `Serialize` impl).
    Serialize { detail: String },
    /// The output stream refused the bytes.
    Write { detail: String },
}

impl FmtError {
    pub(crate) fn write(e: std::io::Error) -> FmtError {
        FmtError::Write {
            detail: e.to_string(),
        }
    }
}

/// Its code and args (decision 14.37); its English is its message in a
/// catalog (`angzarr_presentation::Catalog`), rendered where it is shown.
impl fmt::Display for FmtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        angzarr_diagnostic::display(self, f)
    }
}

impl std::error::Error for FmtError {}

impl Diagnostic for FmtError {
    fn code(&self) -> &'static str {
        match self {
            FmtError::Serialize { .. } => code!("serialize"),
            FmtError::Write { .. } => code!("write"),
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        match self {
            FmtError::Serialize { detail } | FmtError::Write { detail } => {
                vec![("detail", detail.as_str().into())]
            }
        }
    }
}

/// This crate's en-US messages: its package name, and its catalog, each id
/// under that name (written by the build, `angzarr_diagnostic::build`).
pub const EN_US: (&str, &str) = (
    env!("CARGO_PKG_NAME"),
    include_str!(concat!(env!("OUT_DIR"), "/en-US.ftl")),
);

/// One of every diagnostic this crate raises, for the catalog check.
pub fn diagnostic_samples() -> Vec<Box<dyn Diagnostic>> {
    let s = || "x".to_string();
    vec![
        Box::new(FmtError::Serialize { detail: s() }),
        Box::new(FmtError::Write { detail: s() }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The message ids of this crate's catalog.
    fn messages() -> Vec<&'static str> {
        let mut ids: Vec<&str> = EN_US
            .1
            .lines()
            .filter_map(|line| line.split_once(" = ").map(|(id, _)| id))
            .collect();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn every_catalog_message_is_the_code_of_a_sample() {
        let samples = diagnostic_samples();
        let mut codes: Vec<&str> = samples.iter().map(|d| d.code()).collect();
        codes.sort_unstable();
        assert_eq!(codes, messages());
    }

    /// Every error displays as its code and args, never English (decision
    /// 14.37), and core's catalog check agrees with the catalog.
    #[test]
    fn every_error_displays_as_its_code_and_args() {
        let (krate, ftl) = EN_US;
        let problems = angzarr_diagnostic::catalog::check(krate, ftl, &diagnostic_samples());
        assert_eq!(problems, Vec::new());
    }
}
