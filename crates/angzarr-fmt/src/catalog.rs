//! Rendering diagnostics in a language: a code is a Fluent message id, the
//! args fill its variables. A code the catalog lacks, or a message that does
//! not format, falls back to the diagnostic's English `Display`.

use std::collections::BTreeSet;

use angzarr_embedded_diagnostic::{Arg, Diagnostic};
use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use fluent_syntax::ast;
use unic_langid::LanguageIdentifier;

use crate::error::FmtError;

/// This crate's en-US messages.
pub const EN_US: &str = include_str!("../locales/en-US/angzarr-fmt.ftl");

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
        let resource = FluentResource::try_new(ftl.to_string()).map_err(|(_, errors)| {
            FmtError::CatalogParse {
                detail: format!("{errors:?}"),
            }
        })?;
        self.bundle
            .add_resource(resource)
            .map_err(|errors| FmtError::CatalogParse {
                detail: format!("{errors:?}"),
            })
    }

    /// The diagnostic's message in this catalog's language; for a code the
    /// catalog has no message for, or one that fails to render, its code
    /// and args (its `Display`), never English of its own.
    pub fn render(&self, d: &dyn Diagnostic) -> String {
        let Some(pattern) = self.bundle.get_message(d.code()).and_then(|m| m.value()) else {
            return d.to_string();
        };
        let mut args = FluentArgs::new();
        for (name, arg) in d.args() {
            match arg {
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

/// Problems with a catalog against the diagnostics it must cover: a code
/// with no message, or a message whose variables are not exactly the code's
/// args. Empty means the catalog is complete.
pub fn check(ftl: &str, samples: &[Box<dyn Diagnostic>]) -> Vec<String> {
    let resource = match fluent_syntax::parser::parse(ftl) {
        Ok(resource) => resource,
        Err((_, errors)) => return vec![format!("catalog does not parse: {errors:?}")],
    };
    let mut problems = Vec::new();
    for sample in samples {
        let code = sample.code();
        let message = resource.body.iter().find_map(|entry| match entry {
            ast::Entry::Message(m) if m.id.name == code => Some(m),
            _ => None,
        });
        let Some(message) = message else {
            problems.push(format!("{code}: no message"));
            continue;
        };
        let mut used = BTreeSet::new();
        if let Some(pattern) = &message.value {
            pattern_vars(pattern, &mut used);
        }
        let given: BTreeSet<String> = sample
            .args()
            .into_iter()
            .map(|(name, _)| name.to_string())
            .collect();
        if used != given {
            problems.push(format!(
                "{code}: message uses {used:?}, the code gives {given:?}"
            ));
        }
    }
    problems
}

fn pattern_vars(pattern: &ast::Pattern<&str>, out: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let ast::PatternElement::Placeable { expression } = element {
            expression_vars(expression, out);
        }
    }
}

fn expression_vars(expression: &ast::Expression<&str>, out: &mut BTreeSet<String>) {
    match expression {
        ast::Expression::Inline(inline) => inline_vars(inline, out),
        ast::Expression::Select { selector, variants } => {
            inline_vars(selector, out);
            for variant in variants {
                pattern_vars(&variant.value, out);
            }
        }
    }
}

fn inline_vars(inline: &ast::InlineExpression<&str>, out: &mut BTreeSet<String>) {
    match inline {
        ast::InlineExpression::VariableReference { id } => {
            out.insert(id.name.to_string());
        }
        ast::InlineExpression::FunctionReference { arguments, .. } => {
            for positional in &arguments.positional {
                inline_vars(positional, out);
            }
            for named in &arguments.named {
                inline_vars(&named.value, out);
            }
        }
        ast::InlineExpression::Placeable { expression } => expression_vars(expression, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_embedded_diagnostic::Report;

    #[test]
    fn this_crates_catalog_covers_every_diagnostic_with_exactly_its_args() {
        assert_eq!(
            check(EN_US, &crate::error::diagnostic_samples()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_known_code_renders_from_the_catalog_with_its_args() {
        let mut catalog = Catalog::en_us();
        catalog
            .add("greet = hello { $who }, { $n } times\n")
            .unwrap();
        let d = Report::new("greet").arg("who", "world").arg("n", 3u32);
        assert_eq!(catalog.render(&d), "hello world, 3 times");
    }

    /// A code no catalog holds renders as its code and args (decision
    /// 14.37): a diagnostic carries no English to fall back to.
    #[test]
    fn an_unknown_code_falls_back_to_its_code_and_args() {
        let d = Report::new("not-in-catalog").arg("n", 3u32);
        assert_eq!(Catalog::en_us().render(&d), "not-in-catalog n=3");
    }

    #[test]
    fn a_nested_cause_is_rendered_in_its_own_right() {
        let mut catalog = Catalog::en_us();
        catalog
            .add("outer = could not load: { $cause }\ninner = no file { $path }\n")
            .unwrap();
        let inner = Report::new("inner").arg("path", "a.yaml");
        let outer = Report::new("outer").arg("cause", Arg::nested(&inner));
        assert_eq!(catalog.render(&outer), "could not load: no file a.yaml");
    }

    #[test]
    fn a_duplicate_message_id_is_refused() {
        let mut catalog = Catalog::en_us();
        catalog.add("a = one\n").unwrap();
        assert!(catalog.add("a = two\n").is_err());
    }

    #[test]
    fn check_names_a_missing_message_and_mismatched_args() {
        let samples: Vec<Box<dyn Diagnostic>> = vec![
            Box::new(Report::new("missing")),
            Box::new(Report::new("wrong-args").arg("expected", "x")),
        ];
        let problems = check("wrong-args = uses { $other }\n", &samples);
        assert_eq!(problems.len(), 2);
        assert!(problems[0].starts_with("missing: no message"));
        assert!(problems[1].starts_with("wrong-args:"));
    }
}
