//! The output formats and how an invocation picks one.

use std::fmt;
use std::str::FromStr;

use angzarr_embedded_diagnostic::{Arg, Diagnostic};

/// How a result is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Json,
    Text,
}

impl Format {
    /// Every format, in the order help and completion list them.
    pub const ALL: [Format; 2] = [Format::Json, Format::Text];

    /// The name a user types.
    pub fn name(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Text => "text",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Format {
    type Err = UnknownFormat;

    fn from_str(s: &str) -> Result<Format, UnknownFormat> {
        Format::ALL
            .into_iter()
            .find(|f| f.name() == s)
            .ok_or_else(|| UnknownFormat {
                name: s.to_string(),
            })
    }
}

/// The format an invocation writes in. An explicit request always wins;
/// otherwise a terminal gets text and anything else gets `off_terminal`.
pub fn resolve(requested: Option<Format>, terminal: bool, off_terminal: Format) -> Format {
    match requested {
        Some(format) => format,
        None if terminal => Format::Text,
        None => off_terminal,
    }
}

/// A format name that is not one of [`Format::ALL`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownFormat {
    pub name: String,
}

impl fmt::Display for UnknownFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown format {}: use json or text", self.name)
    }
}

impl std::error::Error for UnknownFormat {}

impl Diagnostic for UnknownFormat {
    fn code(&self) -> &'static str {
        "angzarr-fmt-unknown-format"
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        vec![("name", self.name.as_str().into())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_format_wins_on_and_off_a_terminal() {
        for terminal in [true, false] {
            assert_eq!(
                resolve(Some(Format::Json), terminal, Format::Text),
                Format::Json
            );
        }
    }

    #[test]
    fn unset_is_text_on_a_terminal() {
        assert_eq!(resolve(None, true, Format::Json), Format::Text);
    }

    #[test]
    fn unset_off_a_terminal_is_the_configured_default() {
        assert_eq!(resolve(None, false, Format::Json), Format::Json);
        assert_eq!(resolve(None, false, Format::Text), Format::Text);
    }

    #[test]
    fn every_name_parses_back_to_its_format() {
        for format in Format::ALL {
            assert_eq!(format.name().parse::<Format>(), Ok(format));
        }
    }

    #[test]
    fn an_unknown_name_is_refused_with_the_name_as_an_arg() {
        let err = "xml".parse::<Format>().unwrap_err();
        assert_eq!(err.code(), "angzarr-fmt-unknown-format");
        assert!(matches!(&err.args()[0], ("name", Arg::Str(s)) if s == "xml"));
    }
}
