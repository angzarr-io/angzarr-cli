//! The output formats and how an invocation picks one.

use std::fmt;
use std::str::FromStr;

use annals_diagnostic::{Arg, Diagnostic};

/// How a result is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Json,
    Yaml,
    Toml,
    Text,
    Markdown,
}

impl Format {
    /// Every format, in the order help and completion list them.
    pub const ALL: [Format; 5] = [
        Format::Json,
        Format::Yaml,
        Format::Toml,
        Format::Text,
        Format::Markdown,
    ];

    /// The name a user types.
    pub fn name(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Yaml => "yaml",
            Format::Toml => "toml",
            Format::Text => "text",
            Format::Markdown => "markdown",
        }
    }

    /// Whether a program reads this format: the machine shape is the serde
    /// shape, and warnings go out as JSON Lines.
    pub fn is_structured(self) -> bool {
        matches!(self, Format::Json | Format::Yaml | Format::Toml)
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

/// The format a failure or warning is written to stderr in. The structured
/// envelope goes out when the format was asked for (`explicit`) or stderr
/// is not a terminal; a person at a terminal who did not ask gets text.
pub fn diagnostic_format(format: Format, explicit: bool, stderr_terminal: bool) -> Format {
    if explicit || !stderr_terminal {
        format
    } else {
        Format::Text
    }
}

/// A format name that is not one of [`Format::ALL`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownFormat {
    pub name: String,
}

impl fmt::Display for UnknownFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown format {}: use json, yaml, toml, text or markdown",
            self.name
        )
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
                resolve(Some(Format::Yaml), terminal, Format::Json),
                Format::Yaml
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
        assert_eq!(resolve(None, false, Format::Toml), Format::Toml);
    }

    #[test]
    fn a_failure_is_structured_when_asked_for_or_off_a_terminal() {
        assert_eq!(diagnostic_format(Format::Json, true, true), Format::Json);
        assert_eq!(diagnostic_format(Format::Json, false, false), Format::Json);
        assert_eq!(diagnostic_format(Format::Json, false, true), Format::Text);
        assert_eq!(diagnostic_format(Format::Yaml, true, false), Format::Yaml);
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

    #[test]
    fn json_yaml_and_toml_are_structured() {
        let structured: Vec<_> = Format::ALL
            .into_iter()
            .filter(|f| f.is_structured())
            .collect();
        assert_eq!(structured, [Format::Json, Format::Yaml, Format::Toml]);
    }
}
