//! The JSON writer. The machine shape is the serde shape, nothing else;
//! display hints never reach it.

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

#[cfg(test)]
mod tests {
    use super::*;
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
}
