//! Failures: what a command reports when it does not produce its result.

use std::io::Write;

use angzarr_embedded_diagnostic::{Arg, Diagnostic, Report};
use serde_json::{Map, Value, json};

use crate::catalog::Catalog;
use crate::error::FmtError;
use crate::format::Format;
use crate::machine;

/// A diagnostic that ends a command.
pub trait Failure: Diagnostic {
    /// What the user can do about it, as its own localizable message. A
    /// wrapping failure with nothing to add returns its cause's remedy.
    fn remedy(&self) -> Option<Report> {
        None
    }

    /// The process exit status.
    fn exit_code(&self) -> u8 {
        1
    }
}

/// A diagnostic as data: its code, its message in the catalog's language,
/// and its args (a nested cause as an object of the same shape).
pub fn diagnostic_value(catalog: &Catalog, d: &dyn Diagnostic) -> Value {
    json!({
        "code": d.code(),
        "message": catalog.render(d),
        "args": args_value(catalog, d),
    })
}

fn args_value(catalog: &Catalog, d: &dyn Diagnostic) -> Value {
    let mut map = Map::new();
    for (name, arg) in d.args() {
        let value = match arg {
            Arg::Int(n) => json!(n),
            Arg::Uint(n) => json!(n),
            Arg::Str(s) => json!(s),
            Arg::Nested(inner) => diagnostic_value(catalog, &*inner),
        };
        map.insert(name.to_string(), value);
    }
    Value::Object(map)
}

/// The structured shape of a failure: `{"error": {...}, "remedy": {...}}`,
/// each a [`diagnostic_value`]; `remedy` is absent when there is none.
pub fn failure_value(catalog: &Catalog, f: &dyn Failure) -> Value {
    let mut map = Map::new();
    map.insert("error".into(), diagnostic_value(catalog, f));
    if let Some(remedy) = f.remedy() {
        map.insert("remedy".into(), diagnostic_value(catalog, &remedy));
    }
    Value::Object(map)
}

/// Writes a failure in the selected format: the structured shape in json;
/// `Error: ...` and `Fix: ...` lines in text.
pub fn write_failure(
    w: &mut dyn Write,
    format: Format,
    catalog: &Catalog,
    f: &dyn Failure,
) -> Result<(), FmtError> {
    let remedy = f.remedy().map(|r| catalog.render(&r));
    match format {
        Format::Json => machine::write_json(w, &failure_value(catalog, f)),
        Format::Text => {
            writeln!(w, "Error: {}", catalog.render(f)).map_err(FmtError::write)?;
            if let Some(fix) = remedy {
                writeln!(w, "Fix: {fix}").map_err(FmtError::write)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A failure with an arg and a remedy, each with a message in the test
    /// catalog.
    #[derive(Debug)]
    struct Refused;

    impl std::fmt::Display for Refused {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("xml is refused")
        }
    }

    impl std::error::Error for Refused {}

    impl Diagnostic for Refused {
        fn code(&self) -> &'static str {
            "test-refused"
        }

        fn args(&self) -> Vec<(&'static str, Arg)> {
            vec![("name", "xml".into())]
        }
    }

    impl Failure for Refused {
        fn remedy(&self) -> Option<Report> {
            Some(Report::new("test-fix").arg("name", "json"))
        }
    }

    fn catalog() -> Catalog {
        let mut c = Catalog::en_us();
        c.add(crate::catalog::EN_US).unwrap();
        c.add("test-refused = { $name } is refused\ntest-fix = use { $name }\n")
            .unwrap();
        c
    }

    fn written(format: Format, f: &dyn Failure) -> String {
        let mut out = Vec::new();
        write_failure(&mut out, format, &catalog(), f).unwrap();
        String::from_utf8(out).unwrap()
    }

    /// The text with Fluent's isolation marks taken out.
    fn plain(text: &str) -> String {
        text.chars()
            .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
            .collect()
    }

    #[test]
    fn a_failure_in_json_carries_code_message_args_and_remedy() {
        let value: Value = serde_json::from_str(&written(Format::Json, &Refused)).unwrap();
        assert_eq!(value["error"]["code"], "test-refused");
        assert_eq!(value["error"]["args"]["name"], "xml");
        assert_eq!(
            plain(value["error"]["message"].as_str().unwrap()),
            "xml is refused"
        );
        assert_eq!(value["remedy"]["code"], "test-fix");
        assert_eq!(value["remedy"]["args"]["name"], "json");
    }

    #[test]
    fn a_failure_without_a_remedy_has_no_remedy_key() {
        let err = FmtError::Write {
            detail: "broken pipe".into(),
        };
        let value: Value = serde_json::from_str(&written(Format::Json, &err)).unwrap();
        assert!(value.get("remedy").is_none());
    }

    #[test]
    fn a_failure_in_text_is_an_error_line_and_a_fix_line() {
        assert_eq!(
            plain(&written(Format::Text, &Refused)),
            "Error: xml is refused\nFix: use json\n"
        );
    }

    #[test]
    fn a_failure_in_text_without_a_remedy_has_no_fix_line() {
        let err = FmtError::Write {
            detail: "broken pipe".into(),
        };
        assert_eq!(
            plain(&written(Format::Text, &err)),
            "Error: the output could not be written: broken pipe\n"
        );
    }
}
