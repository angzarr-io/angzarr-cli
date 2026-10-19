//! A result that is a list of diagnostics, such as a lint's findings.

use annals_diagnostic::Diagnostic;
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::catalog::Catalog;
use crate::doc::{Block, Doc, NONE};
use crate::error::FmtError;
use crate::failure::diagnostic_value;
use crate::{RenderCtx, Viewable};

/// How much a finding matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    /// The name in every format.
    pub fn name(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// One diagnostic in a [`Diagnostics`] result.
#[derive(Debug)]
pub struct Finding {
    pub severity: Severity,
    /// Where it was found (`path:line:column`, an id), when that is known.
    pub location: Option<String>,
    pub diagnostic: Box<dyn Diagnostic>,
}

/// A list of diagnostics as a command's result. The machine shape is a list
/// of [`diagnostic_value`] objects, each with its `severity` and `location`
/// (null when unknown) added; the human view is a table of severity,
/// location, code and message, or `(none)`.
#[derive(Debug, Default)]
pub struct Diagnostics {
    pub findings: Vec<Finding>,
}

impl Diagnostics {
    /// No findings yet.
    pub fn new() -> Diagnostics {
        Diagnostics::default()
    }

    /// Adds one finding.
    pub fn push(
        &mut self,
        severity: Severity,
        location: Option<String>,
        diagnostic: Box<dyn Diagnostic>,
    ) {
        self.findings.push(Finding {
            severity,
            location,
            diagnostic,
        });
    }

    /// Whether any finding is an error: a command returning these should
    /// exit non-zero.
    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Error)
    }

    /// The machine shape, with messages from `catalog`.
    pub fn value(&self, catalog: &Catalog) -> Value {
        Value::Array(
            self.findings
                .iter()
                .map(|f| {
                    let mut value = diagnostic_value(catalog, &*f.diagnostic);
                    let map = value
                        .as_object_mut()
                        .expect("a diagnostic value is an object");
                    map.insert("severity".into(), f.severity.name().into());
                    map.insert(
                        "location".into(),
                        f.location.clone().map_or(Value::Null, Value::String),
                    );
                    value
                })
                .collect(),
        )
    }
}

/// Without a catalog, every message is the diagnostic's English.
impl Serialize for Diagnostics {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value(&Catalog::en_us()).serialize(serializer)
    }
}

impl Viewable for Diagnostics {
    fn to_value(&self, ctx: &RenderCtx) -> Result<Value, FmtError> {
        Ok(self.value(ctx.catalog))
    }

    fn view(&self, _derived: Doc, ctx: &RenderCtx) -> Doc {
        if self.findings.is_empty() {
            return vec![Block::Para(NONE.to_string())];
        }
        let columns = ["Severity", "Location", "Code", "Message"]
            .map(String::from)
            .to_vec();
        let rows = self
            .findings
            .iter()
            .map(|f| {
                vec![
                    f.severity.name().to_string(),
                    f.location.clone().unwrap_or_else(|| NONE.to_string()),
                    f.diagnostic.code().to_string(),
                    ctx.catalog.render(&*f.diagnostic),
                ]
            })
            .collect();
        vec![Block::Table {
            title: None,
            columns,
            rows,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Format, render};
    use annals_diagnostic::Report;
    use serde_json::json;

    fn findings() -> Diagnostics {
        let mut d = Diagnostics::new();
        d.push(
            Severity::Error,
            Some("a.yaml:3:1".into()),
            Box::new(Report::new("t-bad", "bad thing").arg("n", 2u32)),
        );
        d.push(
            Severity::Warning,
            None,
            Box::new(Report::new("t-meh", "meh")),
        );
        d
    }

    fn rendered(format: Format, d: &Diagnostics) -> String {
        let mut catalog = Catalog::en_us();
        catalog.add("t-bad = { $n } bad things\n").unwrap();
        let mut out = Vec::new();
        render(&mut out, format, &RenderCtx { catalog: &catalog }, d).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn the_machine_shape_is_a_list_of_localized_diagnostics_with_severity_and_location() {
        let value: Value = serde_json::from_str(&rendered(Format::Json, &findings())).unwrap();
        assert_eq!(
            value,
            json!([
                {"code": "t-bad", "message": "2 bad things", "args": {"n": 2},
                 "severity": "error", "location": "a.yaml:3:1"},
                {"code": "t-meh", "message": "meh", "args": {},
                 "severity": "warning", "location": null},
            ])
        );
    }

    #[test]
    fn the_human_view_is_a_table() {
        assert_eq!(
            rendered(Format::Text, &findings()),
            "Severity  Location    Code   Message\n\
             error     a.yaml:3:1  t-bad  2 bad things\n\
             warning   (none)      t-meh  meh\n"
        );
        assert_eq!(rendered(Format::Text, &Diagnostics::new()), "(none)\n");
    }

    #[test]
    fn errors_are_what_make_a_result_fail() {
        assert!(findings().has_errors());
        let mut d = Diagnostics::new();
        d.push(
            Severity::Warning,
            None,
            Box::new(Report::new("t-meh", "meh")),
        );
        assert!(!d.has_errors());
    }

    #[test]
    fn serde_without_a_catalog_uses_the_english() {
        let value = serde_json::to_value(findings()).unwrap();
        assert_eq!(value[0]["message"], "bad thing");
    }
}
