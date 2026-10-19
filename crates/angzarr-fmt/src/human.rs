//! The text and markdown writers. Both lay out the same [`Doc`]; neither
//! bends to the other's shape.

use std::io::{self, Write};

use unicode_width::UnicodeWidthStr;

use crate::doc::{Block, Doc};

/// The lines of a value, whatever its line endings.
fn lines(text: &str) -> Vec<&str> {
    text.split("\r\n")
        .flat_map(|l| l.split(['\n', '\r']))
        .collect()
}

/// Writes a document as plain text: `Label: value` lines, `Title:` headings
/// with their body indented two spaces, and aligned tables. A multi-line
/// value continues indented under its line; a multi-line cell takes extra
/// lines within its row.
pub fn write_text(w: &mut dyn Write, doc: &Doc) -> io::Result<()> {
    text_blocks(w, doc, 0)
}

/// Writes the first line of `value` after `lead`, and every further line indented
/// two past `pad`.
fn text_lines(w: &mut dyn Write, pad: &str, lead: &str, value: &str) -> io::Result<()> {
    let mut parts = lines(value).into_iter();
    writeln!(w, "{pad}{lead}{}", parts.next().unwrap_or_default())?;
    for line in parts {
        writeln!(w, "{pad}  {line}")?;
    }
    Ok(())
}

fn text_blocks(w: &mut dyn Write, doc: &Doc, indent: usize) -> io::Result<()> {
    let pad = " ".repeat(indent);
    for block in doc {
        match block {
            Block::Field { label, value } => text_lines(w, &pad, &format!("{label}: "), value)?,
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
                    text_lines(w, &item_pad, "", item)?;
                }
            }
            Block::Para(text) => {
                for line in lines(text) {
                    writeln!(w, "{pad}{line}")?;
                }
            }
        }
    }
    Ok(())
}

/// The table's rows with the header first, every row as wide as the widest
/// (a column a row has more cells than headers for gets an empty header).
fn grid<'a>(columns: &'a [String], rows: &'a [Vec<String>]) -> Vec<Vec<&'a str>> {
    let width = rows
        .iter()
        .map(Vec::len)
        .chain(std::iter::once(columns.len()))
        .max()
        .unwrap_or(0);
    std::iter::once(columns)
        .chain(rows.iter().map(Vec::as_slice))
        .map(|row| {
            (0..width)
                .map(|i| row.get(i).map_or("", String::as_str))
                .collect()
        })
        .collect()
}

fn text_table(
    w: &mut dyn Write,
    columns: &[String],
    rows: &[Vec<String>],
    indent: usize,
) -> io::Result<()> {
    let grid = grid(columns, rows);
    let count = grid.first().map_or(0, Vec::len);
    let widths: Vec<usize> = (0..count)
        .map(|i| {
            grid.iter()
                .flat_map(|row| lines(row[i]))
                .map(UnicodeWidthStr::width)
                .max()
                .unwrap_or(0)
        })
        .collect();
    let pad = " ".repeat(indent);
    for row in &grid {
        let cells: Vec<Vec<&str>> = row.iter().map(|cell| lines(cell)).collect();
        let height = cells.iter().map(Vec::len).max().unwrap_or(1);
        for n in 0..height {
            let mut line = pad.clone();
            for (i, cell) in cells.iter().enumerate() {
                if i > 0 {
                    line.push_str("  ");
                }
                let part = cell.get(n).copied().unwrap_or("");
                line.push_str(part);
                line.push_str(&" ".repeat(widths[i] - part.width()));
            }
            writeln!(w, "{}", line.trim_end())?;
        }
    }
    Ok(())
}

/// Writes a document as markdown: fields as a bullet list of `**Label:**
/// value`, sections as headings one level deeper per nesting (down to `######`),
/// tables as pipe tables. Every piece of text is escaped so it reads
/// literally; a line break inside a value, item or cell is `<br>`.
pub fn write_markdown(w: &mut dyn Write, doc: &Doc) -> io::Result<()> {
    markdown_blocks(w, doc, 1)
}

/// The deepest markdown heading.
const MAX_HEADING: usize = 6;

fn heading(w: &mut dyn Write, level: usize, title: &str) -> io::Result<()> {
    let hashes = "#".repeat(level.min(MAX_HEADING));
    writeln!(
        w,
        "{hashes} {}\n",
        lines(title)
            .iter()
            .map(|l| escape(l, false))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn markdown_blocks(w: &mut dyn Write, doc: &Doc, level: usize) -> io::Result<()> {
    let mut in_fields = false;
    for block in doc {
        let is_field = matches!(block, Block::Field { .. });
        if in_fields && !is_field {
            writeln!(w)?;
        }
        in_fields = is_field;
        match block {
            Block::Field { label, value } => writeln!(
                w,
                "- **{}:** {}",
                inline(label, false),
                inline(value, false)
            )?,
            Block::Section { title, body } => {
                heading(w, level, title)?;
                markdown_blocks(w, body, level + 1)?;
            }
            Block::Table {
                title,
                columns,
                rows,
            } => {
                if let Some(t) = title {
                    heading(w, level, t)?;
                }
                let grid = grid(columns, rows);
                let row_text = |row: &Vec<&str>| {
                    let cells: Vec<String> = row.iter().map(|c| inline(c, false)).collect();
                    format!("| {} |", cells.join(" | "))
                };
                let mut grid = grid.iter();
                if let Some(header) = grid.next() {
                    writeln!(w, "{}", row_text(header))?;
                    writeln!(w, "|{}|", vec!["---"; header.len()].join("|"))?;
                }
                for row in grid {
                    writeln!(w, "{}", row_text(row))?;
                }
                writeln!(w)?;
            }
            Block::List { title, items } => {
                if let Some(t) = title {
                    heading(w, level, t)?;
                }
                for item in items {
                    writeln!(w, "- {}", inline(item, true))?;
                }
                writeln!(w)?;
            }
            Block::Para(text) => {
                let escaped: Vec<String> = lines(text).iter().map(|l| escape(l, true)).collect();
                writeln!(w, "{}\n", escaped.join("\n"))?
            }
        }
    }
    if in_fields {
        writeln!(w)?;
    }
    Ok(())
}

/// Text on one markdown line: escaped, its line breaks as `<br>`. `starts`
/// says whether it starts the line, where it could open a block.
fn inline(text: &str, starts: bool) -> String {
    lines(text)
        .iter()
        .enumerate()
        .map(|(i, l)| escape(l, starts && i == 0))
        .collect::<Vec<_>>()
        .join("<br>")
}

/// Text that markdown shows literally: every character with inline meaning
/// is backslash-escaped, and, when `starts` (the text begins a line), so is
/// a start that would open a block (a list marker, a numbered item, a
/// setext underline).
fn escape(line: &str, starts: bool) -> String {
    let mut out = String::with_capacity(line.len());
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    for (i, c) in line.char_indices() {
        let opens_block = starts
            && (i == 0 && matches!(c, '-' | '+' | '=')
                || (i == digits && digits > 0 && matches!(c, '.' | ')')));
        if opens_block || "\\`*_[]<>#|~&".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(doc: &Doc) -> String {
        let mut out = Vec::new();
        write_text(&mut out, doc).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn markdown(doc: &Doc) -> String {
        let mut out = Vec::new();
        write_markdown(&mut out, doc).unwrap();
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
    fn markdown_bullets_fields_heads_sections_and_escapes_cells() {
        assert_eq!(
            markdown(&sample()),
            "- **Name:** angzarr\n\n\
             # Build\n\n- **Commit:** abc\n\n\
             # Files\n\n| Path | Policy |\n|---|---|\n| src/lib.rs | regenerate |\n| a\\|b | once |\n\n"
        );
    }

    #[test]
    fn nested_sections_go_one_heading_level_deeper() {
        let doc = vec![Block::Section {
            title: "A".into(),
            body: vec![Block::Section {
                title: "B".into(),
                body: vec![Block::Para("x".into())],
            }],
        }];
        assert_eq!(markdown(&doc), "# A\n\n## B\n\nx\n\n");
    }

    #[test]
    fn an_untitled_list_is_one_item_per_line() {
        let doc = vec![Block::List {
            title: None,
            items: vec!["a".into(), "b".into()],
        }];
        assert_eq!(text(&doc), "a\nb\n");
    }

    fn field(label: &str, value: &str) -> Block {
        Block::Field {
            label: label.into(),
            value: value.into(),
        }
    }

    fn table(columns: &[&str], rows: &[&[&str]]) -> Block {
        Block::Table {
            title: None,
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|c| c.to_string()).collect())
                .collect(),
        }
    }

    const SPECIAL: &str = "*x* <b> _y_ [z](u) # h";
    const ESCAPED: &str = "\\*x\\* \\<b\\> \\_y\\_ \\[z\\](u) \\# h";

    #[test]
    fn markdown_escapes_labels_values_items_paragraphs_and_headings() {
        let doc = vec![
            field(SPECIAL, SPECIAL),
            Block::List {
                title: Some(SPECIAL.into()),
                items: vec![SPECIAL.into(), "- not a sublist".into()],
            },
            Block::Para(SPECIAL.into()),
            Block::Para("1. not a list\n# not a heading".into()),
        ];
        assert_eq!(
            markdown(&doc),
            format!(
                "- **{ESCAPED}:** {ESCAPED}\n\n\
                 # {ESCAPED}\n\n- {ESCAPED}\n- \\- not a sublist\n\n\
                 {ESCAPED}\n\n\
                 1\\. not a list\n\\# not a heading\n\n"
            )
        );
    }

    #[test]
    fn markdown_breaks_multi_line_values_items_and_cells_with_br() {
        let doc = vec![
            field("Note", "one\ntwo"),
            Block::List {
                title: None,
                items: vec!["a\nb".into()],
            },
            table(&["C"], &[&["x\r\ny\rz\nw|v"]]),
        ];
        assert_eq!(
            markdown(&doc),
            "- **Note:** one<br>two\n\n- a<br>b\n\n| C |\n|---|\n| x<br>y<br>z<br>w\\|v |\n\n"
        );
    }

    #[test]
    fn markdown_headings_stop_at_level_six() {
        let mut doc = vec![Block::Para("x".into())];
        for title in ["8", "7", "6", "5", "4", "3", "2", "1"] {
            doc = vec![Block::Section {
                title: title.into(),
                body: doc,
            }];
        }
        assert_eq!(
            markdown(&doc),
            "# 1\n\n## 2\n\n### 3\n\n#### 4\n\n##### 5\n\n###### 6\n\n###### 7\n\n###### 8\n\nx\n\n"
        );
    }

    #[test]
    fn text_indents_the_continuation_of_a_multi_line_value() {
        let doc = vec![Block::Section {
            title: "S".into(),
            body: vec![field("Note", "one\ntwo\r\nthree")],
        }];
        assert_eq!(text(&doc), "S:\n  Note: one\n    two\n    three\n");
    }

    #[test]
    fn text_writes_a_multi_line_cell_as_extra_lines_of_its_row() {
        let doc = vec![table(&["A", "B"], &[&["one\ntwo", "x"], &["3", "y"]])];
        assert_eq!(text(&doc), "A    B\none  x\ntwo\n3    y\n");
    }

    #[test]
    fn text_measures_cells_by_display_width() {
        let doc = vec![table(
            &["A", "B"],
            &[&["\u{4e2d}\u{6587}", "x"], &["e\u{301}", "y"]],
        )];
        assert_eq!(text(&doc), "A     B\n\u{4e2d}\u{6587}  x\ne\u{301}     y\n");
    }

    #[test]
    fn text_and_markdown_keep_every_cell_of_a_ragged_row() {
        let doc = vec![table(&["A", "B"], &[&["1"], &["2", "3", "4"]])];
        assert_eq!(text(&doc), "A  B\n1\n2  3  4\n");
        assert_eq!(
            markdown(&doc),
            "| A | B |  |\n|---|---|---|\n| 1 |  |  |\n| 2 | 3 | 4 |\n\n"
        );
    }
}
