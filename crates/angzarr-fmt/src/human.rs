//! The text writer: it lays out a [`Doc`].

use std::io::{self, Write};

use crate::doc::{Block, Doc};

/// Writes a document as plain text: `Label: value` lines, `Title:` headings
/// with their body indented two spaces, and aligned tables.
pub fn write_text(w: &mut dyn Write, doc: &Doc) -> io::Result<()> {
    text_blocks(w, doc, 0)
}

fn text_blocks(w: &mut dyn Write, doc: &Doc, indent: usize) -> io::Result<()> {
    let pad = " ".repeat(indent);
    for block in doc {
        match block {
            Block::Field { label, value } => writeln!(w, "{pad}{label}: {value}")?,
            Block::Section { title, body } => {
                writeln!(w, "{pad}{title}:")?;
                text_blocks(w, body, indent + 2)?;
            }
            Block::Table {
                title,
                columns,
                rows,
            } => {
                let inner = match title {
                    Some(t) => {
                        writeln!(w, "{pad}{t}:")?;
                        indent + 2
                    }
                    None => indent,
                };
                text_table(w, columns, rows, inner)?;
            }
            Block::List { title, items } => {
                let inner = match title {
                    Some(t) => {
                        writeln!(w, "{pad}{t}:")?;
                        indent + 2
                    }
                    None => indent,
                };
                let item_pad = " ".repeat(inner);
                for item in items {
                    writeln!(w, "{item_pad}{item}")?;
                }
            }
            Block::Para(text) => {
                for line in text.lines() {
                    writeln!(w, "{pad}{line}")?;
                }
            }
        }
    }
    Ok(())
}

fn text_table(
    w: &mut dyn Write,
    columns: &[String],
    rows: &[Vec<String>],
    indent: usize,
) -> io::Result<()> {
    let width = |i: usize| {
        rows.iter()
            .filter_map(|row| row.get(i))
            .chain(std::iter::once(&columns[i]))
            .map(|cell| cell.chars().count())
            .max()
            .unwrap_or(0)
    };
    let widths: Vec<usize> = (0..columns.len()).map(width).collect();
    let pad = " ".repeat(indent);
    let line = |cells: &[String]| {
        let joined: Vec<String> = cells
            .iter()
            .zip(&widths)
            .map(|(cell, &wd)| format!("{cell:<wd$}"))
            .collect();
        format!("{pad}{}", joined.join("  ").trim_end())
    };
    writeln!(w, "{}", line(columns))?;
    for row in rows {
        writeln!(w, "{}", line(row))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(doc: &Doc) -> String {
        let mut out = Vec::new();
        write_text(&mut out, doc).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn sample() -> Doc {
        vec![
            Block::Field {
                label: "Name".into(),
                value: "angzarr".into(),
            },
            Block::Section {
                title: "Build".into(),
                body: vec![Block::Field {
                    label: "Commit".into(),
                    value: "abc".into(),
                }],
            },
            Block::Table {
                title: Some("Files".into()),
                columns: vec!["Path".into(), "Policy".into()],
                rows: vec![
                    vec!["src/lib.rs".into(), "regenerate".into()],
                    vec!["a|b".into(), "once".into()],
                ],
            },
        ]
    }

    #[test]
    fn text_indents_sections_and_aligns_tables() {
        assert_eq!(
            text(&sample()),
            "Name: angzarr\n\
             Build:\n  Commit: abc\n\
             Files:\n  Path        Policy\n  src/lib.rs  regenerate\n  a|b         once\n"
        );
    }

    #[test]
    fn an_untitled_list_is_one_item_per_line() {
        let doc = vec![Block::List {
            title: None,
            items: vec!["a".into(), "b".into()],
        }];
        assert_eq!(text(&doc), "a\nb\n");
    }
}
