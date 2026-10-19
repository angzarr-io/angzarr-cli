//! Failures and warnings: what a command reports when it does not produce
//! its result, and diagnostics that do not stop it.

use std::io::Write;

use annals_diagnostic::{Arg, Diagnostic, Report};
use serde_json::{Map, Value, json};

use crate::catalog::Catalog;
use crate::error::FmtError;
use crate::format::Format;
use crate::{human, machine};

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

/// Writes a failure in the selected format: the structured shape in json,
/// yaml and toml; `Error: ...` and `Fix: ...` lines in text and markdown.
pub fn write_failure(
    w: &mut dyn Write,
    format: Format,
    catalog: &Catalog,
    f: &dyn Failure,
) -> Result<(), FmtError> {
    let remedy = f.remedy().map(|r| catalog.render(&r));
    match format {
        Format::Json => machine::write_json(w, &failure_value(catalog, f)),
        Format::Yaml => machine::write_yaml(w, &failure_value(catalog, f)),
        Format::Toml => machine::write_toml(w, &failure_value(catalog, f)),
        Format::Text => {
            writeln!(w, "Error: {}", catalog.render(f)).map_err(FmtError::write)?;
            if let Some(fix) = remedy {
                writeln!(w, "Fix: {fix}").map_err(FmtError::write)?;
            }
            Ok(())
        }
        Format::Markdown => {
            let mut doc = vec![crate::Block::Field {
                label: "Error".into(),
                value: catalog.render(f),
            }];
            if let Some(fix) = remedy {
                doc.push(crate::Block::Field {
                    label: "Fix".into(),
                    value: fix,
                });
            }
            human::write_markdown(w, &doc).map_err(FmtError::write)
        }
    }
}

/// A diagnostic that does not stop the command.
pub struct Warning<'a> {
    /// The program or component raising it.
    pub prog: &'a str,
    pub diagnostic: &'a dyn Diagnostic,
    pub remedy: Option<Report>,
}

/// Writes one warning. In a structured format it is one compact JSON object
/// per line (JSON Lines) whatever the primary format, because a stream of
/// warnings interleaved with a running command needs a self-delimiting
/// record, and only JSON has one. Otherwise it is `prog: warning: ...` with
/// an indented `fix:` line.
pub fn write_warning(
    w: &mut dyn Write,
    format: Format,
    catalog: &Catalog,
    warning: &Warning<'_>,
) -> Result<(), FmtError> {
    if format.is_structured() {
        let mut record = Map::new();
        record.insert("prog".into(), json!(warning.prog));
        record.insert(
            "warning".into(),
            diagnostic_value(catalog, warning.diagnostic),
        );
        if let Some(remedy) = &warning.remedy {
            record.insert("remedy".into(), diagnostic_value(catalog, remedy));
        }
        let line =
            serde_json::to_string(&Value::Object(record)).map_err(|e| FmtError::Serialize {
                detail: e.to_string(),
            })?;
        writeln!(w, "{line}").map_err(FmtError::write)
    } else {
        writeln!(
            w,
            "{}: warning: {}",
            warning.prog,
            catalog.render(warning.diagnostic)
        )
        .map_err(FmtError::write)?;
        if let Some(remedy) = &warning.remedy {
            writeln!(w, "  fix: {}", catalog.render(remedy)).map_err(FmtError::write)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let mut c = Catalog::en_us();
        c.add(crate::catalog::EN_US).unwrap();
        c
    }

    fn written(format: Format, f: &dyn Failure) -> String {
        let mut out = Vec::new();
        write_failure(&mut out, format, &catalog(), f).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn a_failure_in_json_carries_code_message_args_and_remedy() {
        let err = FmtError::TomlNeedsTable { found: "list" };
        let value: Value = serde_json::from_str(&written(Format::Json, &err)).unwrap();
        assert_eq!(value["error"]["code"], "angzarr-fmt-toml-needs-table");
        assert_eq!(value["error"]["args"]["found"], "list");
        assert_eq!(
            value["error"]["message"],
            "TOML output needs a table at the top level, not a list"
        );
        assert_eq!(value["remedy"]["code"], "angzarr-fmt-use-json-or-yaml");
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
        let err = FmtError::TomlNull {
            path: "files[1]".into(),
        };
        assert_eq!(
            written(Format::Text, &err),
            "Error: TOML has no null: the value at files[1] is empty\n\
             Fix: use --format json or --format yaml for this output\n"
        );
    }

    #[test]
    fn warnings_are_json_lines_in_every_structured_format() {
        let d = FmtError::Write { detail: "x".into() };
        for format in [Format::Json, Format::Yaml, Format::Toml] {
            let mut out = Vec::new();
            let warning = Warning {
                prog: "angzarr",
                diagnostic: &d,
                remedy: None,
            };
            write_warning(&mut out, format, &catalog(), &warning).unwrap();
            let text = String::from_utf8(out).unwrap();
            assert_eq!(text.lines().count(), 1);
            let record: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(record["prog"], "angzarr");
            assert_eq!(record["warning"]["code"], "angzarr-fmt-write");
        }
    }

    #[test]
    fn a_warning_in_text_names_the_program_and_the_fix() {
        let d = FmtError::TomlNeedsTable { found: "list" };
        let mut out = Vec::new();
        let warning = Warning {
            prog: "angzarr",
            diagnostic: &d,
            remedy: d.remedy(),
        };
        write_warning(&mut out, Format::Text, &catalog(), &warning).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "angzarr: warning: TOML output needs a table at the top level, not a list\n  \
             fix: use --format json or --format yaml for this output\n"
        );
    }
}
