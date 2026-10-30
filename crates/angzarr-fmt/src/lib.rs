//! CLI output. A command builds its result once and hands it here; the
//! format decides how the user gets it, so the format is a presentation
//! choice and never a branch in business logic.
//!
//! - **Formats:** json and text. An explicit request wins; otherwise a
//!   terminal gets text and anything else gets the program's off-terminal
//!   default ([`resolve`]).
//! - **Machine shape is the serde shape.** json writes exactly what
//!   `Serialize` produces.
//! - **Human view is a document.** text lays out a [`Doc`] derived from the
//!   serde shape, which a result type may replace or decorate
//!   ([`Viewable`]).
//! - **Failures are diagnostics.** Each has a stable code and named args
//!   rendered through a Fluent [`Catalog`], and may name a remedy
//!   ([`Failure`]).
//!
//! Nothing here knows about angzarr or about any argument parser.

mod catalog;
mod doc;
mod error;
mod failure;
mod format;
mod human;
mod machine;

use std::io::Write;

use serde::Serialize;

pub use catalog::{Catalog, EN_US, check};
pub use doc::{Block, Doc, NONE, derive, humanize};
pub use error::{FmtError, diagnostic_samples};
pub use failure::{Failure, diagnostic_value, failure_value, write_failure};
pub use format::{Format, UnknownFormat, resolve};

/// A command result. The default human view is the one derived from the
/// serde shape; override [`Viewable::view`] to replace or decorate it.
pub trait Viewable: Serialize {
    fn view(&self, derived: Doc) -> Doc {
        derived
    }
}

/// Writes a result in `format`.
pub fn render<T: Viewable + ?Sized>(
    w: &mut dyn Write,
    format: Format,
    result: &T,
) -> Result<(), FmtError> {
    let value = serde_json::to_value(result).map_err(|e| FmtError::Serialize {
        detail: e.to_string(),
    })?;
    match format {
        Format::Json => machine::write_json(w, &value),
        Format::Text => human::write_text(w, &result.view(derive(&value))).map_err(FmtError::write),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Version {
        name: &'static str,
        version: &'static str,
    }

    impl Viewable for Version {}

    #[derive(Serialize)]
    struct Decorated {
        name: &'static str,
    }

    impl Viewable for Decorated {
        fn view(&self, mut derived: Doc) -> Doc {
            derived.push(Block::Para("done".into()));
            derived
        }
    }

    fn rendered<T: Viewable>(format: Format, result: &T) -> String {
        let mut out = Vec::new();
        render(&mut out, format, result).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn one_result_renders_in_both_formats() {
        let v = Version {
            name: "angzarr",
            version: "0.1.0",
        };
        assert_eq!(
            rendered(Format::Json, &v),
            "{\n  \"name\": \"angzarr\",\n  \"version\": \"0.1.0\"\n}\n"
        );
        assert_eq!(
            rendered(Format::Text, &v),
            "Name: angzarr\nVersion: 0.1.0\n"
        );
    }

    #[test]
    fn a_custom_view_changes_the_text_only() {
        let d = Decorated { name: "x" };
        assert_eq!(rendered(Format::Text, &d), "Name: x\ndone\n");
        assert_eq!(rendered(Format::Json, &d), "{\n  \"name\": \"x\"\n}\n");
    }
}
