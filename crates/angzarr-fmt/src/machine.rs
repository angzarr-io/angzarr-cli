//! The structured writers. The machine shape is the serde shape, nothing
//! else; display hints never reach these.

use std::io::Write;

use serde_json::Value;

use crate::error::FmtError;

/// Pretty JSON, one document, newline-terminated.
pub fn write_json(w: &mut dyn Write, value: &Value) -> Result<(), FmtError> {
    let text = serde_json::to_string_pretty(value).map_err(|e| FmtError::Serialize {
        detail: e.to_string(),
    })?;
    writeln!(w, "{text}").map_err(FmtError::write)
}

/// YAML, block style. A string is written plain only when every YAML 1.1
/// and 1.2 core-schema reader reads it back as that same string; anything
/// else (a bool, null, number or timestamp look-alike, an indicator, edge
/// space, a control character, the empty string) is double-quoted.
/// Numbers, bools and null are written as themselves; an empty collection
/// is `{}` or `[]`.
pub fn write_yaml(w: &mut dyn Write, value: &Value) -> Result<(), FmtError> {
    let mut out = String::new();
    if is_block(value) {
        yaml_block(&mut out, value, 0);
    } else {
        out.push_str(&yaml_inline(value));
        out.push('\n');
    }
    w.write_all(out.as_bytes()).map_err(FmtError::write)
}

/// A non-empty collection: written over several lines.
fn is_block(value: &Value) -> bool {
    match value {
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        _ => false,
    }
}

/// Writes a non-empty collection, each line starting at `indent`.
fn yaml_block(out: &mut String, value: &Value, indent: usize) {
    let pad = " ".repeat(indent);
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                out.push_str(&pad);
                out.push_str(&yaml_string(key));
                out.push(':');
                yaml_child(out, v, indent + 2, false);
            }
        }
        Value::Array(items) => {
            for item in items {
                out.push_str(&pad);
                out.push('-');
                yaml_child(out, item, indent + 2, true);
            }
        }
        _ => unreachable!("only collections are written as blocks"),
    }
}

/// The value after `key:` or `-`. A collection under a sequence item starts
/// on the item's own line (`- a: 1`, `- - 1`).
fn yaml_child(out: &mut String, value: &Value, indent: usize, compact: bool) {
    if !is_block(value) {
        out.push(' ');
        out.push_str(&yaml_inline(value));
        out.push('\n');
    } else if compact {
        let mut nested = String::new();
        yaml_block(&mut nested, value, indent);
        out.push(' ');
        out.push_str(&nested[indent..]);
    } else {
        out.push('\n');
        yaml_block(out, value, indent);
    }
}

/// A scalar or an empty collection, on one line.
fn yaml_inline(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => yaml_number(n),
        Value::String(s) => yaml_string(s),
        Value::Array(_) => "[]".into(),
        Value::Object(_) => "{}".into(),
    }
}

/// A float exponent with a dot and a signed exponent (`1.0e+300`), the form
/// YAML 1.1 reads as a float as well as 1.2.
fn yaml_number(n: &serde_json::Number) -> String {
    let text = n.to_string();
    let Some(at) = text.find(['e', 'E']) else {
        return text;
    };
    let (mantissa, exponent) = text.split_at(at);
    let exponent = &exponent[1..];
    let mantissa = if mantissa.contains('.') {
        mantissa.to_string()
    } else {
        format!("{mantissa}.0")
    };
    let sign = if exponent.starts_with(['+', '-']) {
        ""
    } else {
        "+"
    };
    format!("{mantissa}e{sign}{exponent}")
}

fn yaml_string(s: &str) -> String {
    if plain_ok(s) {
        return s.to_string();
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            c if needs_escape(c) => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A character a double-quoted scalar writes as an escape: controls, the
/// line and paragraph separators, and the byte order mark.
fn needs_escape(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}')
}

/// Whether `s` reads back as itself when written plain, under the YAML 1.1
/// types (bool, null, int, float, timestamp, merge, value) and the 1.2 core
/// schema. Conservative: a string that only might be retyped is quoted.
fn plain_ok(s: &str) -> bool {
    let Some(first) = s.chars().next() else {
        return false;
    };
    if s.starts_with([' ', '\t']) || s.ends_with([' ', '\t']) || s.chars().any(needs_escape) {
        return false;
    }
    // Indicators that start some other node, quoted scalar or document marker.
    if "-?:,[]{}#&*!|>'\"%@`".contains(first) || s.starts_with("...") {
        return false;
    }
    if s.contains(": ") || s.contains(" #") || s.ends_with(':') {
        return false;
    }
    // A quote after a space or flow indicator opens a quoted scalar for some
    // readers.
    if s.as_bytes()
        .windows(2)
        .any(|w| matches!(w[0], b' ' | b'[' | b'{' | b',') && matches!(w[1], b'"' | b'\''))
    {
        return false;
    }
    let lower = s.to_ascii_lowercase();
    const WORDS: [&str; 13] = [
        "y", "n", "yes", "no", "true", "false", "on", "off", "null", "~", "<<", "=", ".nan",
    ];
    if WORDS.contains(&lower.as_str()) {
        return false;
    }
    // Every int, float, sexagesimal and timestamp form starts with a digit,
    // or a sign or dot before a digit, or is a signed or unsigned `.inf`.
    let unsigned = lower.strip_prefix(['+', '-']).unwrap_or(&lower);
    let undotted = unsigned.strip_prefix('.').unwrap_or(unsigned);
    !(undotted.starts_with(|c: char| c.is_ascii_digit()) || unsigned == ".inf")
}

/// TOML. A document must be a table, TOML has no null (a null field is left
/// out, a null inside a list is refused), and its integers are 64-bit signed
/// (a larger one is refused).
pub fn write_toml(w: &mut dyn Write, value: &Value) -> Result<(), FmtError> {
    if !value.is_object() {
        return Err(FmtError::TomlNeedsTable { found: kind(value) });
    }
    let stripped = strip_nulls(value, "")?;
    let text = toml::to_string_pretty(&stripped).map_err(|e| FmtError::TomlEncode {
        detail: e.to_string(),
    })?;
    w.write_all(text.as_bytes()).map_err(FmtError::write)
}

fn strip_nulls(value: &Value, path: &str) -> Result<Value, FmtError> {
    Ok(match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if !v.is_null() {
                    out.insert(k.clone(), strip_nulls(v, &join(path, k))?);
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(i, v)| match v {
                    Value::Null => Err(FmtError::TomlNull {
                        path: format!("{path}[{i}]"),
                    }),
                    other => strip_nulls(other, &format!("{path}[{i}]")),
                })
                .collect::<Result<_, _>>()?,
        ),
        Value::Number(n) if n.is_u64() && n.as_i64().is_none() => {
            return Err(FmtError::TomlIntRange {
                path: path.to_string(),
            });
        }
        scalar => scalar.clone(),
    })
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "list",
        Value::Object(_) => "table",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use annals_diagnostic::Diagnostic;
    use serde_json::json;

    fn run(
        f: fn(&mut dyn Write, &Value) -> Result<(), FmtError>,
        v: &Value,
    ) -> Result<String, FmtError> {
        let mut out = Vec::new();
        f(&mut out, v)?;
        Ok(String::from_utf8(out).unwrap())
    }

    #[test]
    fn json_keeps_field_order() {
        let out = run(write_json, &json!({"z": 1, "a": 2})).unwrap();
        assert!(out.find("\"z\"").unwrap() < out.find("\"a\"").unwrap());
        assert!(out.ends_with("}\n"));
    }

    #[test]
    fn yaml_writes_block_style_and_reads_back() {
        let out = run(
            write_yaml,
            &json!({"name": "angzarr", "langs": ["rust", "python"], "commit": null}),
        )
        .unwrap();
        let back = annals_yaml::parse(&out).unwrap();
        assert_eq!(
            back.get("name").and_then(annals_yaml::Value::as_str),
            Some("angzarr")
        );
        assert_eq!(back.get("commit"), Some(&annals_yaml::Value::Null));
    }

    /// Every string a YAML 1.1 or 1.2 core-schema resolver would read as
    /// something else (or as a different string) is double-quoted, and reads
    /// back as the same string.
    #[test]
    fn yaml_quotes_every_string_a_typed_reader_would_retype() {
        let tricky = [
            "true",
            "True",
            "FALSE",
            "yes",
            "No",
            "on",
            "OFF",
            "y",
            "n",
            "123",
            "-7",
            "+1",
            "1.10",
            "0.1.0",
            "0123456",
            "0x1f",
            "0o17",
            "0b101",
            "1_000",
            "1:20",
            "1e3",
            ".5",
            ".inf",
            "-.inf",
            ".NaN",
            "null",
            "Null",
            "~",
            "",
            "2026-10-18",
            "2026-10-18T10:00:00Z",
            " lead",
            "trail ",
            "a: b",
            "a #b",
            "end:",
            "- x",
            "? x",
            "[x]",
            "{x}",
            "#x",
            "&x",
            "*x",
            "!x",
            "|x",
            ">x",
            "'x",
            "\"x",
            "%x",
            "@x",
            "`x",
            ",x",
            "...",
            "<<",
            "=",
            "line\nbreak",
            "tab\there",
            "nul\u{0}",
            "bell\u{7}",
            "del\u{7f}",
            "nel\u{85}",
            "ls\u{2028}",
            "bom\u{feff}",
        ];
        for s in tricky {
            let out = run(write_yaml, &json!({ "k": s })).unwrap();
            assert!(out.starts_with("k: \""), "{s:?} written plain: {out:?}");
            let back = annals_yaml::parse(&out).unwrap();
            assert_eq!(
                back.get("k").and_then(annals_yaml::Value::as_str),
                Some(s),
                "{s:?} did not read back: {out:?}"
            );
        }
        let out = run(write_yaml, &json!({ "yes": 1 })).unwrap();
        assert_eq!(out, "\"yes\": 1\n");
    }

    #[test]
    fn yaml_writes_ordinary_strings_plain_and_typed_values_typed() {
        let out = run(
            write_yaml,
            &json!({"name": "angzarr", "path": "src/lib.rs", "count": 3, "f": 1.5, "ok": true,
                    "none": null, "big": 1e300}),
        )
        .unwrap();
        assert_eq!(
            out,
            "name: angzarr\npath: src/lib.rs\ncount: 3\nf: 1.5\nok: true\nnone: null\nbig: 1.0e+300\n"
        );
    }

    #[test]
    fn yaml_nests_block_collections_and_writes_empties_in_flow() {
        let out = run(
            write_yaml,
            &json!({"build": {"commit": "abc"}, "langs": ["rust", "go"], "files": [
                {"path": "a", "tags": []}, {"path": "b", "meta": {}}
            ], "grid": [[1, 2], []], "empty": {}}),
        )
        .unwrap();
        assert_eq!(
            out,
            "build:\n  commit: abc\nlangs:\n  - rust\n  - go\nfiles:\n  - path: a\n    tags: []\n  \
             - path: b\n    meta: {}\ngrid:\n  - - 1\n    - 2\n  - []\nempty: {}\n"
        );
        annals_yaml::parse(&out).unwrap();
        assert_eq!(run(write_yaml, &json!([])).unwrap(), "[]\n");
        assert_eq!(run(write_yaml, &json!("yes")).unwrap(), "\"yes\"\n");
    }

    #[test]
    fn toml_leaves_out_null_fields() {
        let out = run(write_toml, &json!({"version": "0.1.0", "commit": null})).unwrap();
        assert_eq!(out, "version = \"0.1.0\"\n");
    }

    #[test]
    fn toml_refuses_a_document_that_is_not_a_table() {
        let err = run(write_toml, &json!(["a"])).unwrap_err();
        assert_eq!(err, FmtError::TomlNeedsTable { found: "list" });
        assert_eq!(err.code(), "angzarr-fmt-toml-needs-table");
    }

    #[test]
    fn toml_refuses_an_integer_beyond_its_range_and_names_where() {
        let err = run(write_toml, &json!({"a": {"n": u64::MAX}})).unwrap_err();
        assert_eq!(err, FmtError::TomlIntRange { path: "a.n".into() });
        assert_eq!(err.code(), "angzarr-fmt-toml-int-range");
        let ok = run(write_toml, &json!({"n": i64::MAX as u64})).unwrap();
        assert_eq!(ok, format!("n = {}\n", i64::MAX));
    }

    #[test]
    fn toml_refuses_null_in_a_list_and_names_where() {
        let err = run(write_toml, &json!({"files": [{"path": null}, null]})).unwrap_err();
        assert_eq!(
            err,
            FmtError::TomlNull {
                path: "files[1]".into()
            }
        );
    }
}
