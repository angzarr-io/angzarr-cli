//! The human view of a result: a document of blocks that the text and
//! markdown writers lay out. Derivation builds one from the serde shape; a
//! result type may replace or decorate it (see [`crate::Viewable`]).

use serde_json::{Map, Value};

/// A human view: blocks in order.
pub type Doc = Vec<Block>;

/// One element of a [`Doc`]. The set is closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// One `Label: value` line.
    Field { label: String, value: String },
    /// A titled, nested document.
    Section { title: String, body: Doc },
    /// An aligned table: one cell per column in every row.
    Table {
        title: Option<String>,
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// A run of items, one per line.
    List {
        title: Option<String>,
        items: Vec<String>,
    },
    /// Verbatim text.
    Para(String),
}

/// What an absent or empty value reads as.
pub const NONE: &str = "(none)";

/// The view derived from a serialized value. Objects become fields and
/// sections, lists of scalars a joined field (or a list at the top), lists of
/// records with the same keys a table, other lists one section per item.
pub fn derive(value: &Value) -> Doc {
    match value {
        Value::Object(map) => fields(map),
        Value::Array(items) => list(None, items),
        scalar => vec![Block::Para(scalar_text(scalar))],
    }
}

fn fields(map: &Map<String, Value>) -> Doc {
    let mut doc = Vec::new();
    for (key, value) in map {
        let label = humanize(key);
        match value {
            Value::Object(inner) if inner.is_empty() => doc.push(Block::Field {
                label,
                value: NONE.to_string(),
            }),
            Value::Object(inner) => doc.push(Block::Section {
                title: label,
                body: fields(inner),
            }),
            Value::Array(items) => doc.extend(list(Some(label), items)),
            scalar => doc.push(Block::Field {
                label,
                value: scalar_text(scalar),
            }),
        }
    }
    doc
}

fn list(title: Option<String>, items: &[Value]) -> Doc {
    if items.is_empty() {
        return vec![match title {
            Some(label) => Block::Field {
                label,
                value: NONE.to_string(),
            },
            None => Block::Para(NONE.to_string()),
        }];
    }
    if items.iter().all(is_scalar) {
        let texts: Vec<String> = items.iter().map(scalar_text).collect();
        return vec![match title {
            Some(label) => Block::Field {
                label,
                value: texts.join(", "),
            },
            None => Block::List {
                title: None,
                items: texts,
            },
        }];
    }
    if let Some(keys) = shared_keys(items) {
        let rows = items
            .iter()
            .filter_map(Value::as_object)
            .map(|record| {
                keys.iter()
                    .map(|k| cell_text(&record[k.as_str()]))
                    .collect()
            })
            .collect();
        let columns = keys.iter().map(|k| humanize(k)).collect();
        return vec![Block::Table {
            title,
            columns,
            rows,
        }];
    }
    let item_title = |i: usize| match &title {
        Some(t) => format!("{t} {}", i + 1),
        None => format!("Item {}", i + 1),
    };
    items
        .iter()
        .enumerate()
        .map(|(i, item)| Block::Section {
            title: item_title(i),
            body: derive(item),
        })
        .collect()
}

/// The keys every item has, in the first item's order, when every item is a
/// record with exactly those keys.
fn shared_keys(items: &[Value]) -> Option<Vec<String>> {
    let first: Vec<String> = items.first()?.as_object()?.keys().cloned().collect();
    if first.is_empty() {
        return None;
    }
    let same = items.iter().all(|item| {
        item.as_object().is_some_and(|record| {
            record.len() == first.len() && first.iter().all(|k| record.contains_key(k))
        })
    });
    same.then_some(first)
}

fn is_scalar(value: &Value) -> bool {
    !matches!(value, Value::Array(_) | Value::Object(_))
}

fn scalar_text(value: &Value) -> String {
    match value {
        Value::Null => NONE.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A table cell: a scalar as text, anything nested inline as compact JSON.
fn cell_text(value: &Value) -> String {
    if is_scalar(value) {
        scalar_text(value)
    } else {
        value.to_string()
    }
}

/// A serialized key as a label: separators become spaces and the first
/// letter is capitalized (`schema_version` reads `Schema version`).
pub fn humanize(key: &str) -> String {
    let spaced: String = key
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect();
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => spaced,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn field(label: &str, value: &str) -> Block {
        Block::Field {
            label: label.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn a_record_is_fields_in_serde_order_with_humanized_labels() {
        let doc = derive(&json!({"name": "angzarr", "schema_version": 2, "dirty": false}));
        assert_eq!(
            doc,
            [
                field("Name", "angzarr"),
                field("Schema version", "2"),
                field("Dirty", "false")
            ]
        );
    }

    #[test]
    fn a_nested_record_is_a_section() {
        let doc = derive(&json!({"build": {"commit": "abc"}}));
        assert_eq!(
            doc,
            [Block::Section {
                title: "Build".into(),
                body: vec![field("Commit", "abc")]
            }]
        );
    }

    #[test]
    fn null_and_empty_collections_read_as_none() {
        let doc = derive(&json!({"commit": null, "tags": [], "extra": {}}));
        assert_eq!(
            doc,
            [
                field("Commit", NONE),
                field("Tags", NONE),
                field("Extra", NONE)
            ]
        );
    }

    #[test]
    fn a_list_of_scalars_is_joined_in_a_record_and_a_list_at_the_top() {
        assert_eq!(
            derive(&json!({"langs": ["rust", "python"]})),
            [field("Langs", "rust, python")]
        );
        assert_eq!(
            derive(&json!(["rust", "python"])),
            [Block::List {
                title: None,
                items: vec!["rust".into(), "python".into()]
            }]
        );
    }

    #[test]
    fn records_with_the_same_keys_are_a_table_with_nested_values_inline() {
        let doc = derive(&json!({"files": [
            {"path": "a.rs", "meta": {"n": 1}},
            {"path": "b.rs", "meta": {"n": 2}},
        ]}));
        assert_eq!(
            doc,
            [Block::Table {
                title: Some("Files".into()),
                columns: vec!["Path".into(), "Meta".into()],
                rows: vec![
                    vec!["a.rs".into(), "{\"n\":1}".into()],
                    vec!["b.rs".into(), "{\"n\":2}".into()]
                ],
            }]
        );
    }

    #[test]
    fn records_with_different_keys_are_one_section_each() {
        let doc = derive(&json!([{"a": 1}, {"b": 2}]));
        assert_eq!(
            doc,
            [
                Block::Section {
                    title: "Item 1".into(),
                    body: vec![field("A", "1")]
                },
                Block::Section {
                    title: "Item 2".into(),
                    body: vec![field("B", "2")]
                },
            ]
        );
    }

    #[test]
    fn a_top_level_scalar_is_a_paragraph() {
        assert_eq!(derive(&json!("done")), [Block::Para("done".into())]);
    }

    #[test]
    fn humanize_handles_hyphens_and_the_empty_key() {
        assert_eq!(humanize("off-terminal"), "Off terminal");
        assert_eq!(humanize(""), "");
    }
}
