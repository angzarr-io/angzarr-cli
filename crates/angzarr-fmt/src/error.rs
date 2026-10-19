//! What can go wrong while writing output.

use std::fmt;

use annals_diagnostic::{Arg, Diagnostic, Report};

use crate::failure::Failure;

/// A result that could not be written in the selected format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FmtError {
    /// The result did not serialize (a map with non-string keys, a failing
    /// `Serialize` impl). `detail` is the serializer's own text.
    Serialize { detail: String },
    /// TOML needs a table at the top level. `found` is what the top level
    /// is: `list`, `string`, `number`, `boolean` or `null`.
    TomlNeedsTable { found: &'static str },
    /// TOML has no null, and a list cannot leave an item out.
    TomlNull { path: String },
    /// TOML integers are 64-bit signed; this one is larger.
    TomlIntRange { path: String },
    /// The TOML encoder refused the value. `detail` is the encoder's own
    /// text.
    TomlEncode { detail: String },
    /// The output stream refused the bytes. `kind` is the I/O error kind as
    /// a kebab-case token (`broken-pipe`), see [`io_kind`].
    Write { kind: &'static str },
    /// A message catalog did not parse. `line` and `column` (1-based, the
    /// column in characters) locate the first error; `kind` is the parser's
    /// error kind as a kebab-case token (`expected-token`).
    CatalogParse {
        line: u64,
        column: u64,
        kind: String,
    },
    /// Two catalogs define the same message id.
    CatalogDuplicate { id: String },
}

impl FmtError {
    pub(crate) fn write(e: std::io::Error) -> FmtError {
        FmtError::Write {
            kind: io_kind(e.kind()),
        }
    }
}

/// An I/O error kind as a stable kebab-case token; a kind not listed here is
/// `other`.
pub fn io_kind(kind: std::io::ErrorKind) -> &'static str {
    use std::io::ErrorKind as K;
    match kind {
        K::NotFound => "not-found",
        K::PermissionDenied => "permission-denied",
        K::ConnectionRefused => "connection-refused",
        K::ConnectionReset => "connection-reset",
        K::ConnectionAborted => "connection-aborted",
        K::NotConnected => "not-connected",
        K::BrokenPipe => "broken-pipe",
        K::AlreadyExists => "already-exists",
        K::WouldBlock => "would-block",
        K::InvalidInput => "invalid-input",
        K::InvalidData => "invalid-data",
        K::TimedOut => "timed-out",
        K::WriteZero => "write-zero",
        K::Interrupted => "interrupted",
        K::Unsupported => "unsupported",
        K::UnexpectedEof => "unexpected-eof",
        K::OutOfMemory => "out-of-memory",
        K::StorageFull => "storage-full",
        K::ReadOnlyFilesystem => "read-only-filesystem",
        K::IsADirectory => "is-a-directory",
        K::NotADirectory => "not-a-directory",
        _ => "other",
    }
}

impl From<std::io::Error> for FmtError {
    fn from(e: std::io::Error) -> FmtError {
        FmtError::write(e)
    }
}

impl fmt::Display for FmtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FmtError::Serialize { detail } => {
                write!(f, "the result could not be serialized: {detail}")
            }
            FmtError::TomlNeedsTable { found } => {
                let what = if *found == "null" {
                    "null".to_string()
                } else {
                    format!("a {found}")
                };
                write!(f, "TOML output needs a table at the top level, not {what}")
            }
            FmtError::TomlNull { path } => {
                write!(f, "TOML has no null: the value at {path} is empty")
            }
            FmtError::TomlIntRange { path } => write!(
                f,
                "TOML integers are 64-bit signed: the value at {path} is too large"
            ),
            FmtError::TomlEncode { detail } => {
                write!(f, "the result could not be written as TOML: {detail}")
            }
            FmtError::Write { kind } => write!(f, "the output could not be written: {kind}"),
            FmtError::CatalogParse { line, column, kind } => write!(
                f,
                "a message catalog has a syntax error at line {line}, column {column}: {kind}"
            ),
            FmtError::CatalogDuplicate { id } => {
                write!(f, "two message catalogs define {id}")
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
            FmtError::TomlIntRange { .. } => "angzarr-fmt-toml-int-range",
            FmtError::TomlEncode { .. } => "angzarr-fmt-toml-encode",
            FmtError::Write { .. } => "angzarr-fmt-write",
            FmtError::CatalogParse { .. } => "angzarr-fmt-catalog-parse",
            FmtError::CatalogDuplicate { .. } => "angzarr-fmt-catalog-duplicate",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        match self {
            FmtError::Serialize { detail } | FmtError::TomlEncode { detail } => {
                vec![("detail", detail.as_str().into())]
            }
            FmtError::TomlNeedsTable { found } => vec![("found", (*found).into())],
            FmtError::TomlNull { path } | FmtError::TomlIntRange { path } => {
                vec![("path", path.as_str().into())]
            }
            FmtError::Write { kind } => vec![("kind", (*kind).into())],
            FmtError::CatalogParse { line, column, kind } => vec![
                ("line", (*line).into()),
                ("column", (*column).into()),
                ("kind", kind.as_str().into()),
            ],
            FmtError::CatalogDuplicate { id } => vec![("id", id.as_str().into())],
        }
    }
}

impl Failure for FmtError {
    fn remedy(&self) -> Option<Report> {
        match self {
            FmtError::TomlNeedsTable { .. }
            | FmtError::TomlNull { .. }
            | FmtError::TomlIntRange { .. } => Some(Report::new(
                "angzarr-fmt-use-json-or-yaml",
                "write this output as json or yaml instead",
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
        Box::new(FmtError::TomlIntRange { path: s() }),
        Box::new(FmtError::TomlEncode { detail: s() }),
        Box::new(FmtError::Write {
            kind: "broken-pipe",
        }),
        Box::new(FmtError::CatalogParse {
            line: 1,
            column: 1,
            kind: s(),
        }),
        Box::new(FmtError::CatalogDuplicate { id: s() }),
        Box::new(crate::format::UnknownFormat { name: s() }),
    ];
    samples.extend(
        FmtError::TomlNeedsTable { found: "list" }
            .remedy()
            .map(|r| Box::new(r) as Box<dyn Diagnostic>),
    );
    samples
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, EN_US};

    fn arg<'a>(args: &'a [(&'static str, Arg)], name: &str) -> &'a Arg {
        &args.iter().find(|(n, _)| *n == name).unwrap().1
    }

    #[test]
    fn a_write_failure_carries_the_io_kind_as_a_token() {
        let err = FmtError::write(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        assert_eq!(err.code(), "angzarr-fmt-write");
        let args = err.args();
        assert!(matches!(arg(&args, "kind"), Arg::Str(s) if s == "broken-pipe"));
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn a_catalog_syntax_error_names_its_line_column_and_kind() {
        let err = Catalog::en_us()
            .add("a = one\nb = { $x ! }\nc = c\n")
            .unwrap_err();
        assert_eq!(err.code(), "angzarr-fmt-catalog-parse");
        let args = err.args();
        assert!(matches!(arg(&args, "line"), Arg::Uint(2)), "{args:?}");
        assert!(matches!(arg(&args, "column"), Arg::Uint(10)), "{args:?}");
        assert!(
            matches!(arg(&args, "kind"), Arg::Str(s) if s.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')),
            "{args:?}"
        );
    }

    #[test]
    fn a_message_id_given_twice_is_its_own_code_naming_the_id() {
        let mut catalog = Catalog::en_us();
        catalog.add("a = one\n").unwrap();
        let err = catalog.add("a = two\n").unwrap_err();
        assert_eq!(
            err,
            FmtError::CatalogDuplicate {
                id: "a".to_string()
            }
        );
    }

    #[test]
    fn toml_needs_table_selects_its_wording_by_the_kind_token() {
        let mut catalog = Catalog::en_us();
        catalog.add(EN_US).unwrap();
        for (found, text) in [
            ("list", "a list"),
            ("string", "a string"),
            ("number", "a number"),
            ("boolean", "a boolean"),
            ("null", "null"),
        ] {
            assert_eq!(
                catalog.render(&FmtError::TomlNeedsTable { found }),
                format!("TOML output needs a table at the top level, not {text}")
            );
        }
    }

    #[test]
    fn every_listed_io_kind_has_its_own_token() {
        use std::io::ErrorKind as K;
        for (kind, token) in [
            (K::NotFound, "not-found"),
            (K::PermissionDenied, "permission-denied"),
            (K::ConnectionRefused, "connection-refused"),
            (K::ConnectionReset, "connection-reset"),
            (K::ConnectionAborted, "connection-aborted"),
            (K::NotConnected, "not-connected"),
            (K::BrokenPipe, "broken-pipe"),
            (K::AlreadyExists, "already-exists"),
            (K::WouldBlock, "would-block"),
            (K::InvalidInput, "invalid-input"),
            (K::InvalidData, "invalid-data"),
            (K::TimedOut, "timed-out"),
            (K::WriteZero, "write-zero"),
            (K::Interrupted, "interrupted"),
            (K::Unsupported, "unsupported"),
            (K::UnexpectedEof, "unexpected-eof"),
            (K::OutOfMemory, "out-of-memory"),
            (K::StorageFull, "storage-full"),
            (K::ReadOnlyFilesystem, "read-only-filesystem"),
            (K::IsADirectory, "is-a-directory"),
            (K::NotADirectory, "not-a-directory"),
            (K::Other, "other"),
            (K::AddrInUse, "other"),
        ] {
            assert_eq!(io_kind(kind), token, "{kind:?}");
        }
    }

    #[test]
    fn the_english_fallback_for_toml_needs_table_has_no_article_before_null() {
        assert_eq!(
            FmtError::TomlNeedsTable { found: "null" }.to_string(),
            "TOML output needs a table at the top level, not null"
        );
        assert_eq!(
            FmtError::TomlNeedsTable { found: "list" }.to_string(),
            "TOML output needs a table at the top level, not a list"
        );
    }

    #[test]
    fn the_remedy_names_formats_not_a_flag() {
        let mut catalog = Catalog::en_us();
        catalog.add(EN_US).unwrap();
        let remedy = FmtError::TomlIntRange { path: "n".into() }
            .remedy()
            .unwrap();
        assert_eq!(
            catalog.render(&remedy),
            "write this output as json or yaml instead"
        );
        assert!(!remedy.to_string().contains("--"));
    }
}
