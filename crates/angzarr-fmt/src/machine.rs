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

/// YAML through annals-yaml's writer, which writes a scalar plain only when
/// it reads back unchanged.
pub fn write_yaml(w: &mut dyn Write, value: &Value) -> Result<(), FmtError> {
    // Small collections of scalars go on one line once nested two levels.
    const FLOW_DEPTH: usize = 2;
    w.write_all(annals_yaml::write(&to_yaml(value), FLOW_DEPTH).as_bytes())
        .map_err(FmtError::write)
}

fn to_yaml(value: &Value) -> annals_yaml::Value {
    match value {
        Value::Null => annals_yaml::Value::Null,
        Value::Bool(b) => annals_yaml::Value::scalar(b.to_string()),
        Value::Number(n) => annals_yaml::Value::scalar(n.to_string()),
        Value::String(s) => annals_yaml::Value::scalar(s.clone()),
        Value::Array(items) => annals_yaml::Value::Seq(items.iter().map(to_yaml).collect()),
        Value::Object(map) => {
            annals_yaml::Value::Map(map.iter().map(|(k, v)| (k.clone(), to_yaml(v))).collect())
        }
    }
}

/// TOML. A document must be a table, and TOML has no null: a null field is
/// left out, a null inside a list is refused.
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
