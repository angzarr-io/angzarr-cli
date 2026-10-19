//! CLI output. A command builds its result once and hands it here; the
//! format decides how the user gets it, so the format is a presentation
//! choice and never a branch in business logic.
//!
//! - **Formats:** json, yaml, toml, text and markdown. An explicit request
//!   wins; otherwise a terminal gets text and anything else gets the
//!   program's off-terminal default ([`resolve`]).
//! - **Machine shape is the serde shape.** json, yaml and toml write exactly
//!   what `Serialize` produces, unless the result supplies its own value
//!   ([`Viewable::to_value`]), as one with localized content does.
//! - **Human view is a document.** text and markdown lay out a [`Doc`]
//!   derived from the machine shape, which a result type may replace or
//!   decorate ([`Viewable`]). Both get a [`RenderCtx`] holding the catalog.
//! - **Findings are a result.** A command whose result is a list of
//!   diagnostics (a lint) returns [`Diagnostics`]: each with a severity and
//!   an optional location, localized in every format.
//! - **Failures and warnings are diagnostics.** Each has a stable code and
//!   named args rendered through a Fluent [`Catalog`], and a failure may name
//!   a remedy ([`Failure`]). Warnings in a structured format are JSON Lines.
//!
//! Nothing here knows about angzarr or about any argument parser.

mod catalog;
mod diagnostics;
mod doc;
mod error;
mod failure;
mod format;
mod human;
mod machine;

use std::io::Write;

use serde::Serialize;

pub use annals_diagnostic::{Arg, Diagnostic, Report};
pub use catalog::{Catalog, EN_US, check};
pub use diagnostics::{Diagnostics, Finding, Severity};
pub use doc::{Block, Doc, NONE, derive, humanize};
pub use error::{FmtError, diagnostic_samples};
pub use failure::{
    Failure, Warning, diagnostic_value, failure_value, write_failure, write_warning,
};
pub use format::{Format, UnknownFormat, diagnostic_format, resolve};

/// What rendering a result may draw on besides the result itself.
#[derive(Clone, Copy)]
pub struct RenderCtx<'a> {
    /// The messages diagnostics are rendered with.
    pub catalog: &'a Catalog,
}

/// A command result. Its machine shape is its serde shape unless
/// [`Viewable::to_value`] says otherwise (to localize content, say); its
/// human view is the one derived from that shape unless [`Viewable::view`]
/// replaces or decorates it.
pub trait Viewable: Serialize {
    /// The machine shape, which the human view is also derived from.
    fn to_value(&self, ctx: &RenderCtx) -> Result<serde_json::Value, FmtError> {
        let _ = ctx;
        serde_json::to_value(self).map_err(|e| FmtError::Serialize {
            detail: e.to_string(),
        })
    }

    /// The human view, given the one derived from the machine shape.
    fn view(&self, derived: Doc, ctx: &RenderCtx) -> Doc {
        let _ = ctx;
        derived
    }
}

/// Writes a result in `format`.
pub fn render<T: Viewable + ?Sized>(
    w: &mut dyn Write,
    format: Format,
    ctx: &RenderCtx,
    result: &T,
) -> Result<(), FmtError> {
    let value = result.to_value(ctx)?;
    match format {
        Format::Json => machine::write_json(w, &value),
        Format::Yaml => machine::write_yaml(w, &value),
        Format::Toml => machine::write_toml(w, &value),
        Format::Text => {
            human::write_text(w, &result.view(derive(&value), ctx)).map_err(FmtError::write)
        }
        Format::Markdown => {
            human::write_markdown(w, &result.view(derive(&value), ctx)).map_err(FmtError::write)
        }
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
        fn view(&self, mut derived: Doc, _: &RenderCtx) -> Doc {
            derived.push(Block::Para("done".into()));
            derived
        }
    }

    /// A result whose content is a diagnostic, localized in every format.
    #[derive(Serialize)]
    struct Localized {
        #[serde(skip)]
        report: annals_diagnostic::Report,
    }

    impl Viewable for Localized {
        fn to_value(&self, ctx: &RenderCtx) -> Result<serde_json::Value, FmtError> {
            Ok(serde_json::json!({ "said": ctx.catalog.render(&self.report) }))
        }
    }

    fn catalog() -> Catalog {
        let mut c = Catalog::en_us();
        c.add(EN_US).unwrap();
        c.add("t-hi = bonjour { $who }\n").unwrap();
        c
    }

    fn rendered<T: Viewable>(format: Format, result: &T) -> String {
        let catalog = catalog();
        let mut out = Vec::new();
        render(&mut out, format, &RenderCtx { catalog: &catalog }, result).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn a_result_may_serialize_localized_content() {
        let l = Localized {
            report: annals_diagnostic::Report::new("t-hi", "hello").arg("who", "x"),
        };
        assert_eq!(
            rendered(Format::Json, &l),
            "{\n  \"said\": \"bonjour x\"\n}\n"
        );
        assert_eq!(rendered(Format::Text, &l), "Said: bonjour x\n");
    }

    #[test]
    fn an_empty_record_reads_as_none_in_text_and_markdown() {
        #[derive(Serialize)]
        struct Empty {}
        impl Viewable for Empty {}
        assert_eq!(rendered(Format::Text, &Empty {}), "(none)\n");
        assert_eq!(rendered(Format::Markdown, &Empty {}), "(none)\n\n");
    }

    #[test]
    fn one_result_renders_in_every_format() {
        let v = Version {
            name: "angzarr",
            version: "0.1.0",
        };
        assert_eq!(
            rendered(Format::Json, &v),
            "{\n  \"name\": \"angzarr\",\n  \"version\": \"0.1.0\"\n}\n"
        );
        assert_eq!(
            rendered(Format::Yaml, &v),
            "name: angzarr\nversion: \"0.1.0\"\n"
        );
        assert_eq!(
            rendered(Format::Toml, &v),
            "name = \"angzarr\"\nversion = \"0.1.0\"\n"
        );
        assert_eq!(
            rendered(Format::Text, &v),
            "Name: angzarr\nVersion: 0.1.0\n"
        );
        assert_eq!(
            rendered(Format::Markdown, &v),
            "- **Name:** angzarr\n- **Version:** 0.1.0\n\n"
        );
    }

    #[test]
    fn a_custom_view_changes_the_human_formats_only() {
        let d = Decorated { name: "x" };
        assert_eq!(rendered(Format::Text, &d), "Name: x\ndone\n");
        assert_eq!(rendered(Format::Json, &d), "{\n  \"name\": \"x\"\n}\n");
    }
}
