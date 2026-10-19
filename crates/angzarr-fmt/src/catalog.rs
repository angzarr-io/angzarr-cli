//! Rendering diagnostics in a language: a code is a Fluent message id, the
//! args fill its variables. A code the catalog lacks, or a message that does
//! not format, falls back to the diagnostic's English `Display`.

use std::collections::{BTreeMap, BTreeSet};

use annals_diagnostic::{Arg, Diagnostic};
use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use fluent_syntax::ast;
use unic_langid::LanguageIdentifier;

use crate::error::FmtError;

/// This crate's en-US messages.
pub const EN_US: &str = include_str!("../locales/en-US/angzarr-fmt.ftl");

/// The largest integer a Fluent number (an `f64`) holds exactly. A larger
/// one goes to Fluent as its digits, so none is lost.
const EXACT: u64 = 1 << 53;

/// Messages for one language, gathered from every crate's catalog.
pub struct Catalog {
    bundle: FluentBundle<FluentResource>,
}

impl Catalog {
    /// An empty en-US catalog.
    pub fn en_us() -> Catalog {
        let lang: LanguageIdentifier = "en-US"
            .parse()
            .expect("en-US is a valid language identifier");
        let mut bundle = FluentBundle::new(vec![lang]);
        // Isolation marks are for mixed-direction UI text; a terminal shows
        // them as stray characters.
        bundle.set_use_isolating(false);
        Catalog { bundle }
    }

    /// Adds one crate's messages. A later message with an existing id is a
    /// catalog bug and is refused.
    pub fn add(&mut self, ftl: &str) -> Result<(), FmtError> {
        let resource = FluentResource::try_new(ftl.to_string())
            .map_err(|(_, errors)| parse_error(ftl, &errors))?;
        self.bundle.add_resource(resource).map_err(|errors| {
            let id = errors
                .iter()
                .find_map(|e| match e {
                    fluent_bundle::FluentError::Overriding { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            FmtError::CatalogDuplicate { id }
        })
    }

    /// The diagnostic's message in this catalog's language.
    pub fn render(&self, d: &dyn Diagnostic) -> String {
        let Some(pattern) = self.bundle.get_message(d.code()).and_then(|m| m.value()) else {
            return d.to_string();
        };
        let mut args = FluentArgs::new();
        for (name, arg) in d.args() {
            match arg {
                Arg::Int(n) if n.unsigned_abs() > EXACT => args.set(name, n.to_string()),
                Arg::Uint(n) if n > EXACT => args.set(name, n.to_string()),
                Arg::Int(n) => args.set(name, n),
                Arg::Uint(n) => args.set(name, n),
                Arg::Str(s) => args.set(name, s),
                Arg::Nested(inner) => args.set(name, self.render(&*inner)),
            }
        }
        let mut errors = Vec::new();
        let text = self
            .bundle
            .format_pattern(pattern, Some(&args), &mut errors);
        if errors.is_empty() {
            text.into_owned()
        } else {
            d.to_string()
        }
    }
}

/// The first syntax error, located by line and column.
fn parse_error(ftl: &str, errors: &[fluent_syntax::parser::ParserError]) -> FmtError {
    let Some(first) = errors.first() else {
        return FmtError::CatalogParse {
            line: 1,
            column: 1,
            kind: "unknown".into(),
        };
    };
    let at = first.pos.start.min(ftl.len());
    let before = ftl.get(..at).unwrap_or(ftl);
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    FmtError::CatalogParse {
        line: line as u64,
        column: column as u64,
        kind: kebab(&format!("{:?}", first.kind)),
    }
}

/// A Rust variant name (`ExpectedToken('a')`) as a kebab-case token
/// (`expected-token`).
fn kebab(debug: &str) -> String {
    let name = debug.split(['(', ' ', '{']).next().unwrap_or(debug);
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else if c.is_ascii_alphanumeric() {
            out.push(c);
        }
    }
    out
}

/// Problems with one crate's catalog against the diagnostics it must cover.
/// Empty means the catalog is complete. The rules:
///
/// - The catalog parses, with no junk, no id twice, and a value for every
///   message. A message names its own args: a message reference is refused.
/// - Every code is kebab-case and starts with `prefix-`, has a message whose
///   variables (in selects, function and term args, and attributes) are
///   exactly its args, gives each arg once, and has a non-empty English
///   `Display`.
/// - A nested cause whose code starts with `prefix-` is checked in its own
///   right; another crate's cause is that crate's catalog's to cover.
/// - Every message belongs to some diagnostic.
pub fn check(ftl: &str, prefix: &str, samples: &[Box<dyn Diagnostic>]) -> Vec<String> {
    let mut problems = Vec::new();
    let messages = read_messages(ftl, &mut problems);
    let mut seen = BTreeSet::new();
    for sample in samples {
        check_one(&**sample, prefix, &messages, &mut problems, &mut seen);
    }
    for id in messages.keys() {
        if !seen.contains(id.as_str()) {
            problems.push(format!("{id}: no diagnostic has this code"));
        }
    }
    problems
}

/// Message id to the variables its value and attributes use.
type Messages = BTreeMap<String, BTreeSet<String>>;

fn read_messages(ftl: &str, problems: &mut Vec<String>) -> Messages {
    let resource = match fluent_syntax::parser::parse(ftl) {
        Ok(resource) => resource,
        Err((resource, errors)) => {
            for e in &errors {
                problems.push(format!("catalog syntax error: {e:?}"));
            }
            resource
        }
    };
    let mut messages = Messages::new();
    for entry in &resource.body {
        match entry {
            ast::Entry::Message(m) => {
                let id = m.id.name;
                let mut vars = BTreeSet::new();
                let mut walk = Walk {
                    id,
                    vars: &mut vars,
                    problems,
                };
                match &m.value {
                    Some(value) => walk.pattern(value),
                    None => walk.problems.push(format!("{id} has no value")),
                }
                for attribute in &m.attributes {
                    walk.pattern(&attribute.value);
                }
                if messages.insert(id.to_string(), vars).is_some() {
                    problems.push(format!("{id} twice"));
                }
            }
            ast::Entry::Junk { content } => problems.push(format!("junk {content:?}")),
            ast::Entry::Term(_)
            | ast::Entry::Comment(_)
            | ast::Entry::GroupComment(_)
            | ast::Entry::ResourceComment(_) => {}
        }
    }
    messages
}

fn check_one(
    d: &dyn Diagnostic,
    prefix: &str,
    messages: &Messages,
    problems: &mut Vec<String>,
    seen: &mut BTreeSet<&'static str>,
) {
    let code = d.code();
    seen.insert(code);
    if d.to_string().trim().is_empty() {
        problems.push(format!("{code}: empty English Display"));
    }
    if !code_ok(code, prefix) {
        problems.push(format!("{code}: not kebab-case starting `{prefix}-`"));
    }
    let args = d.args();
    let mut given = BTreeSet::new();
    for (name, _) in &args {
        if !given.insert(name.to_string()) {
            problems.push(format!("{code}: arg {name} twice"));
        }
    }
    match messages.get(code) {
        None => problems.push(format!("{code}: no message")),
        Some(used) if *used != given => problems.push(format!(
            "{code}: message uses {used:?}, the code gives {given:?}"
        )),
        Some(_) => {}
    }
    for (_, arg) in &args {
        if let Arg::Nested(inner) = arg
            && inner.code().starts_with(&format!("{prefix}-"))
        {
            check_one(&**inner, prefix, messages, problems, seen);
        }
    }
}

/// kebab-case words of lowercase ASCII letters and digits after `prefix-`.
fn code_ok(code: &str, prefix: &str) -> bool {
    code.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|rest| {
            !rest.is_empty()
                && rest.split('-').all(|word| {
                    !word.is_empty()
                        && word
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                })
        })
}

/// Collects one message's variables, refusing message references.
struct Walk<'a> {
    id: &'a str,
    vars: &'a mut BTreeSet<String>,
    problems: &'a mut Vec<String>,
}

impl Walk<'_> {
    fn pattern(&mut self, pattern: &ast::Pattern<&str>) {
        for element in &pattern.elements {
            if let ast::PatternElement::Placeable { expression } = element {
                self.expression(expression);
            }
        }
    }

    fn expression(&mut self, expression: &ast::Expression<&str>) {
        match expression {
            ast::Expression::Inline(inline) => self.inline(inline),
            ast::Expression::Select { selector, variants } => {
                self.inline(selector);
                for variant in variants {
                    self.pattern(&variant.value);
                }
            }
        }
    }

    fn arguments(&mut self, arguments: &ast::CallArguments<&str>) {
        for positional in &arguments.positional {
            self.inline(positional);
        }
        for named in &arguments.named {
            self.inline(&named.value);
        }
    }

    fn inline(&mut self, inline: &ast::InlineExpression<&str>) {
        match inline {
            ast::InlineExpression::VariableReference { id } => {
                self.vars.insert(id.name.to_string());
            }
            ast::InlineExpression::FunctionReference { arguments, .. } => {
                self.arguments(arguments);
            }
            ast::InlineExpression::TermReference { arguments, .. } => {
                if let Some(arguments) = arguments {
                    self.arguments(arguments);
                }
            }
            ast::InlineExpression::MessageReference { id, .. } => self.problems.push(format!(
                "{}: references message {}; name the args instead",
                self.id, id.name
            )),
            ast::InlineExpression::Placeable { expression } => self.expression(expression),
            ast::InlineExpression::StringLiteral { .. }
            | ast::InlineExpression::NumberLiteral { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use annals_diagnostic::Report;

    #[test]
    fn this_crates_catalog_covers_every_diagnostic_with_exactly_its_args() {
        assert_eq!(
            check(EN_US, "angzarr-fmt", &crate::error::diagnostic_samples()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_known_code_renders_from_the_catalog_with_its_args() {
        let mut catalog = Catalog::en_us();
        catalog
            .add("greet = hello { $who }, { $n } times\n")
            .unwrap();
        let d = Report::new("greet", "english fallback")
            .arg("who", "world")
            .arg("n", 3u32);
        assert_eq!(catalog.render(&d), "hello world, 3 times");
    }

    #[test]
    fn numbers_select_plurals_and_large_ones_keep_every_digit() {
        let mut catalog = Catalog::en_us();
        catalog
            .add("t-n = { $n ->\n    [one] one file\n   *[other] { $n } files\n }\n")
            .unwrap();
        let n = |v: Arg| Report::new("t-n", "n").arg("n", v);
        assert_eq!(catalog.render(&n(Arg::Uint(1))), "one file");
        assert_eq!(catalog.render(&n(Arg::Int(3))), "3 files");
        assert_eq!(
            catalog.render(&n(Arg::Uint(u64::MAX))),
            "18446744073709551615 files"
        );
        assert_eq!(
            catalog.render(&n(Arg::Int(-9_007_199_254_740_993))),
            "-9007199254740993 files"
        );
        assert_eq!(
            catalog.render(&n(Arg::Uint(9_007_199_254_740_992))),
            "9007199254740992 files"
        );
    }

    #[test]
    fn an_unknown_code_falls_back_to_the_english() {
        let d = Report::new("not-in-catalog", "english fallback");
        assert_eq!(Catalog::en_us().render(&d), "english fallback");
    }

    #[test]
    fn a_nested_cause_is_rendered_in_its_own_right() {
        let mut catalog = Catalog::en_us();
        catalog
            .add("outer = could not load: { $cause }\ninner = no file { $path }\n")
            .unwrap();
        let inner = Report::new("inner", "no file").arg("path", "a.yaml");
        let outer = Report::new("outer", "outer").arg("cause", Arg::nested(&inner));
        assert_eq!(catalog.render(&outer), "could not load: no file a.yaml");
    }

    #[test]
    fn a_duplicate_message_id_is_refused() {
        let mut catalog = Catalog::en_us();
        catalog.add("a = one\n").unwrap();
        assert!(catalog.add("a = two\n").is_err());
    }

    fn problems(ftl: &str, samples: Vec<Box<dyn Diagnostic>>) -> Vec<String> {
        check(ftl, "t", &samples)
    }

    fn has(problems: &[String], needle: &str) -> bool {
        problems.iter().any(|p| p.contains(needle))
    }

    #[test]
    fn check_names_a_missing_message_and_mismatched_args() {
        let p = problems(
            "t-wrong-args = uses { $other }\n",
            vec![
                Box::new(Report::new("t-missing", "m")),
                Box::new(Report::new("t-wrong-args", "w").arg("expected", "x")),
            ],
        );
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p[0].starts_with("t-missing: no message"));
        assert!(p[1].starts_with("t-wrong-args:"));
    }

    #[test]
    fn check_refuses_a_message_reference() {
        let p = problems(
            "t-a = see { t-b }\nt-b = b\n",
            vec![
                Box::new(Report::new("t-a", "a")),
                Box::new(Report::new("t-b", "b")),
            ],
        );
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(has(&p, "t-a: references message t-b"));
    }

    #[test]
    fn check_reads_variables_in_selects_functions_term_args_and_attributes() {
        let ftl = "t-a = { $x ->\n    [one] one { $y }\n   *[other] { NUMBER($z) } { -t($w) }\n }\n    .tip = { $v }\n-t = term\n";
        let mut sample = Report::new("t-a", "a");
        for name in ["v", "w", "x", "y", "z"] {
            sample = sample.arg(name, "1");
        }
        assert_eq!(problems(ftl, vec![Box::new(sample)]), Vec::<String>::new());
    }

    #[test]
    fn check_flags_junk_duplicate_ids_and_messages_without_a_value() {
        let p = problems(
            "t-a = one\nt-a = two\nt-b =\n    .attr = x\nthis is not fluent\n",
            vec![
                Box::new(Report::new("t-a", "a")),
                Box::new(Report::new("t-b", "b")),
            ],
        );
        assert!(has(&p, "t-a twice"), "{p:?}");
        assert!(has(&p, "t-b has no value"), "{p:?}");
        assert!(has(&p, "junk"), "{p:?}");
    }

    #[test]
    fn check_flags_a_message_no_diagnostic_has() {
        let p = problems(
            "t-a = a\nt-orphan = o\n",
            vec![Box::new(Report::new("t-a", "a"))],
        );
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(has(&p, "t-orphan: no diagnostic has this code"));
    }

    #[test]
    fn check_flags_codes_that_are_not_kebab_case_under_the_prefix() {
        let ftl = "t-Upper = u\nt--double = d\nother-x = o\nt = bare\n";
        let p = problems(
            ftl,
            vec![
                Box::new(Report::new("t-Upper", "u")),
                Box::new(Report::new("t--double", "d")),
                Box::new(Report::new("other-x", "o")),
                Box::new(Report::new("t", "bare")),
            ],
        );
        for code in ["t-Upper", "t--double", "other-x", "t:"] {
            assert!(has(&p, code), "{code} not flagged: {p:?}");
        }
        assert_eq!(p.len(), 4, "{p:?}");
    }

    #[test]
    fn check_flags_an_arg_given_twice_and_an_empty_english() {
        let p = problems(
            "t-a = { $x }\nt-b = b\n",
            vec![
                Box::new(Report::new("t-a", "a").arg("x", "1").arg("x", "2")),
                Box::new(Report::new("t-b", " ")),
            ],
        );
        assert!(has(&p, "t-a: arg x twice"), "{p:?}");
        assert!(has(&p, "t-b: empty English"), "{p:?}");
    }

    #[test]
    fn check_follows_nested_causes_under_the_prefix() {
        let inner = Report::new("t-inner", "i").arg("path", "p");
        let foreign = Report::new("other-inner", "o");
        let outer = Report::new("t-outer", "o")
            .arg("cause", Arg::nested(&inner))
            .arg("other", Arg::nested(&foreign));
        let p = problems(
            "t-outer = { $cause } { $other }\n",
            vec![Box::new(outer.clone())],
        );
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].starts_with("t-inner: no message"));
        let p = problems(
            "t-outer = { $cause } { $other }\nt-inner = { $path }\n",
            vec![Box::new(outer)],
        );
        assert_eq!(p, Vec::<String>::new());
    }

    #[test]
    fn check_reports_a_syntax_error() {
        let p = problems("t-a = { $x\n", vec![]);
        assert!(!p.is_empty());
    }
}
