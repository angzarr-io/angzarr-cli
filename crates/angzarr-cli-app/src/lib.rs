//! The angzarr CLI's use cases. Each loads a descriptor set through the
//! [`DescriptorSource`] port, drives one run of the core's annals domain in
//! memory, and returns its result as data for `angzarr-fmt` to write.
//!
//! - [`lint`]: every finding, as a [`LintReport`] (`{"diagnostics": [...]}`).
//! - [`model`]: the component model, as a [`ModelReport`].

use std::fmt;

use angzarr_cli_core::DescriptorSet;
use angzarr_cli_core::lint::{Finding, Severity as LintSeverity};
use angzarr_cli_core::model::ComponentModel;
use angzarr_cli_core::run::{self, Refused, Run, RunCommand, RunReport};
use angzarr_fmt::{
    Block, Catalog, Diagnostics, Doc, Failure, FmtError, NONE, RenderCtx, Severity, Viewable,
    diagnostic_value,
};
use annals::{Stream, project};
use annals_diagnostic::{Arg, Diagnostic, Report};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};

/// This crate's en-US messages.
pub const EN_US: &str = include_str!("../locales/en-US/angzarr-cli-app.ftl");

/// Where a descriptor set comes from.
pub trait DescriptorSource {
    /// Loads the set, or says why it could not.
    fn load(&self) -> Result<DescriptorSet, Box<dyn Failure>>;
}

/// A use case could not finish its run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppError {
    /// The run refused one of its own steps: a bug in the use case.
    RunRefused { reason: Refused },
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::RunRefused { reason } => write!(f, "the run refused a step: {reason}"),
        }
    }
}

impl std::error::Error for AppError {}

impl Diagnostic for AppError {
    fn code(&self) -> &'static str {
        match self {
            AppError::RunRefused { .. } => "angzarr-cli-app-run-refused",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        match self {
            AppError::RunRefused { reason } => vec![("reason", Arg::nested(reason))],
        }
    }
}

impl Failure for AppError {}

/// One of every diagnostic this crate raises, for the catalog check.
pub fn diagnostic_samples() -> Vec<Box<dyn Diagnostic>> {
    vec![Box::new(AppError::RunRefused {
        reason: Refused::NoModel,
    })]
}

/// Runs `commands` in a fresh in-memory run over `set`.
fn drive(set: DescriptorSet, commands: &[RunCommand]) -> Result<RunReport, AppError> {
    let mut stream = Stream::<Run>::new(run::header());
    let mut ctx = set;
    for cmd in commands {
        stream
            .execute(cmd, &(), &mut ctx)
            .map_err(|reason| AppError::RunRefused { reason })?;
    }
    let mut report = RunReport::default();
    project(&mut report, stream.events());
    Ok(report)
}

/// Lints the descriptor set `source` gives.
pub fn lint(source: &dyn DescriptorSource) -> Result<LintReport, Box<dyn Failure>> {
    let set = source.load()?;
    let report = drive(set, &[RunCommand::ObserveModel, RunCommand::Lint])?;
    Ok(LintReport::new(report.findings))
}

/// The component model of the descriptor set `source` gives.
pub fn model(source: &dyn DescriptorSource) -> Result<ModelReport, Box<dyn Failure>> {
    let set = source.load()?;
    let report = drive(set, &[RunCommand::ObserveModel])?;
    Ok(ModelReport(report.model.unwrap_or_default()))
}

/// A lint's findings. The machine shape is `{"diagnostics": [...]}`, a
/// table at the top so every format (TOML too) can hold it; each item is a
/// diagnostic with its `severity`, its `location` and, when there is one, a
/// `remedy` of the same shape. The human view is one table.
#[derive(Debug)]
pub struct LintReport {
    pub diagnostics: Diagnostics,
    /// The remedy of each finding, in order.
    pub remedies: Vec<Option<Report>>,
}

impl LintReport {
    pub fn new(findings: Vec<Finding>) -> LintReport {
        let mut diagnostics = Diagnostics::new();
        let mut remedies = Vec::new();
        for f in findings {
            let severity = match f.severity() {
                LintSeverity::Error => Severity::Error,
                LintSeverity::Warning => Severity::Warning,
            };
            remedies.push(f.lint.remedy());
            diagnostics.push(severity, Some(f.location.to_string()), Box::new(f.lint));
        }
        LintReport {
            diagnostics,
            remedies,
        }
    }

    /// Whether any finding is an error: the command then exits non-zero.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }

    /// The machine shape, with messages from `catalog`.
    pub fn value(&self, catalog: &Catalog) -> Value {
        let mut items = match self.diagnostics.value(catalog) {
            Value::Array(items) => items,
            _ => unreachable!("diagnostics are a list"),
        };
        for (item, remedy) in items.iter_mut().zip(&self.remedies) {
            if let (Some(map), Some(remedy)) = (item.as_object_mut(), remedy) {
                map.insert("remedy".into(), diagnostic_value(catalog, remedy));
            }
        }
        json!({ "diagnostics": items })
    }
}

/// Without a catalog, every message is the diagnostic's English.
impl Serialize for LintReport {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value(&Catalog::en_us()).serialize(serializer)
    }
}

impl Viewable for LintReport {
    fn to_value(&self, ctx: &RenderCtx) -> Result<Value, FmtError> {
        Ok(self.value(ctx.catalog))
    }

    fn view(&self, _derived: Doc, ctx: &RenderCtx) -> Doc {
        let findings = &self.diagnostics.findings;
        if findings.is_empty() {
            return vec![Block::Para(NONE.to_string())];
        }
        let columns = ["Severity", "Location", "Code", "Message", "Fix"]
            .map(String::from)
            .to_vec();
        let rows = findings
            .iter()
            .zip(&self.remedies)
            .map(|(f, remedy)| {
                vec![
                    f.severity.name().to_string(),
                    f.location.clone().unwrap_or_else(|| NONE.to_string()),
                    f.diagnostic.code().to_string(),
                    ctx.catalog.render(&*f.diagnostic),
                    remedy
                        .as_ref()
                        .map_or_else(String::new, |r| ctx.catalog.render(r)),
                ]
            })
            .collect();
        vec![Block::Table {
            title: None,
            columns,
            rows,
        }]
    }
}

/// The component model as a result: the machine shape is the model's
/// serde shape; the human view is one section per component.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct ModelReport(pub ComponentModel);

fn field(label: &str, value: impl Into<String>) -> Block {
    Block::Field {
        label: label.to_string(),
        value: value.into(),
    }
}

fn yes_no(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

impl Viewable for ModelReport {
    fn view(&self, _derived: Doc, _ctx: &RenderCtx) -> Doc {
        if self.0.components.is_empty() {
            return vec![Block::Para(NONE.to_string())];
        }
        self.0
            .components
            .iter()
            .map(|c| {
                let mut body = vec![
                    field("Kind", c.kind.token()),
                    field("Anchor", c.anchor.clone()),
                    field("File", c.file.clone()),
                ];
                if let Some(d) = &c.domain {
                    body.push(field("Domain", d.clone()));
                }
                if let Some(d) = &c.input_domain {
                    body.push(field("Input domain", d.clone()));
                }
                if !c.output_domains.is_empty() {
                    body.push(field("Output domains", c.output_domains.join(", ")));
                }
                if let Some(s) = &c.state {
                    let listed = if s.conflict_resolutions.is_empty() {
                        NONE.to_string()
                    } else {
                        s.conflict_resolutions.join(", ")
                    };
                    body.push(field("Conflict resolutions", listed));
                }
                if !c.commands.is_empty() {
                    body.push(Block::Table {
                        title: Some("Commands".into()),
                        columns: vec!["Type".into(), "Emits".into()],
                        rows: c
                            .commands
                            .iter()
                            .map(|cmd| {
                                let emits = if cmd.emits.is_empty() {
                                    NONE.to_string()
                                } else {
                                    cmd.emits.join(", ")
                                };
                                vec![cmd.type_name.clone(), emits]
                            })
                            .collect(),
                    });
                }
                if !c.events.is_empty() {
                    body.push(Block::Table {
                        title: Some("Events".into()),
                        columns: vec!["Type".into(), "Domain".into(), "Applies".into()],
                        rows: c
                            .events
                            .iter()
                            .map(|e| {
                                vec![
                                    e.type_name.clone(),
                                    e.domain.clone().unwrap_or_else(|| NONE.to_string()),
                                    yes_no(e.applies).to_string(),
                                ]
                            })
                            .collect(),
                    });
                }
                if !c.conflict_units.is_empty() {
                    body.push(Block::Table {
                        title: Some("Conflict units".into()),
                        columns: vec!["Message".into(), "Field".into(), "Unit".into()],
                        rows: c
                            .conflict_units
                            .iter()
                            .map(|u| {
                                let mut how = Vec::new();
                                if u.descend {
                                    how.push("descend".to_string());
                                }
                                if let Some(k) = &u.element_key {
                                    how.push(format!("element_key={k}"));
                                }
                                if u.ordered {
                                    how.push("ordered".to_string());
                                }
                                if u.whole_state {
                                    how.push("whole_state".to_string());
                                }
                                let how = if how.is_empty() {
                                    "field".to_string()
                                } else {
                                    how.join(", ")
                                };
                                vec![u.message.clone(), u.field.clone(), how]
                            })
                            .collect(),
                    });
                }
                Block::Section {
                    title: c.name.clone(),
                    body,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_cli_core::fixture::*;
    use angzarr_fmt::{Format, render};

    struct Fixed(Image);

    impl DescriptorSource for Fixed {
        fn load(&self) -> Result<DescriptorSet, Box<dyn Failure>> {
            Ok(self.0.set())
        }
    }

    #[derive(Debug)]
    struct Unreadable;

    impl fmt::Display for Unreadable {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("unreadable")
        }
    }

    impl Diagnostic for Unreadable {
        fn code(&self) -> &'static str {
            "t-unreadable"
        }
        fn args(&self) -> Vec<(&'static str, Arg)> {
            Vec::new()
        }
    }

    impl Failure for Unreadable {}

    struct Broken;

    impl DescriptorSource for Broken {
        fn load(&self) -> Result<DescriptorSet, Box<dyn Failure>> {
            Err(Box::new(Unreadable))
        }
    }

    fn catalog() -> Catalog {
        let mut c = Catalog::en_us();
        c.add(angzarr_fmt::EN_US).unwrap();
        c.add(angzarr_cli_core::EN_US).unwrap();
        c.add(EN_US).unwrap();
        c
    }

    fn rendered<T: Viewable>(format: Format, result: &T) -> String {
        let catalog = catalog();
        let mut out = Vec::new();
        render(&mut out, format, &RenderCtx { catalog: &catalog }, result).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn missing_domain() -> Image {
        Image::new().file(
            File::new("k.proto", "k.v1").message(Msg::new("A").component(component(
                ComponentKind::Aggregate,
                "",
                "",
                &[],
            ))),
        )
    }

    #[test]
    fn the_catalog_has_every_code_with_exactly_its_args() {
        let problems = angzarr_fmt::check(EN_US, "angzarr-cli-app", &diagnostic_samples());
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn a_refused_step_renders_its_reason() {
        let err = AppError::RunRefused {
            reason: Refused::NoModel,
        };
        assert_eq!(
            catalog().render(&err),
            "the run refused a step: the model must be observed before the lint"
        );
    }

    #[test]
    fn a_clean_set_lints_to_no_diagnostics() {
        let report = lint(&Fixed(Image::new().file(counter()))).unwrap();
        assert!(!report.has_errors());
        let value: Value = serde_json::from_str(&rendered(Format::Json, &report)).unwrap();
        assert_eq!(value, json!({"diagnostics": []}));
        assert_eq!(rendered(Format::Text, &report), "(none)\n");
        assert_eq!(rendered(Format::Toml, &report), "diagnostics = []\n");
    }

    #[test]
    fn a_finding_carries_severity_location_and_remedy_in_every_format() {
        let report = lint(&Fixed(missing_domain())).unwrap();
        assert!(report.has_errors());
        let value: Value = serde_json::from_str(&rendered(Format::Json, &report)).unwrap();
        assert_eq!(
            value,
            json!({"diagnostics": [{
                "code": "angzarr-cli-core-missing-field",
                "message": "k.v1.A: an aggregate requires (component).domain",
                "args": {"component": "k.v1.A", "kind": "aggregate", "field": "domain"},
                "severity": "error",
                "location": "k.proto#k.v1.A",
                "remedy": {
                    "code": "angzarr-cli-core-fix-set-field",
                    "message": "set (component).domain",
                    "args": {"field": "domain"},
                },
            }]})
        );
        let toml = rendered(Format::Toml, &report);
        assert!(toml.contains("[[diagnostics]]"), "{toml}");
        let text = rendered(Format::Text, &report);
        assert!(text.starts_with("Severity  Location"), "{text}");
        assert!(text.contains("set (component).domain"), "{text}");
    }

    #[test]
    fn the_model_use_case_returns_the_model() {
        let report = model(&Fixed(Image::new().file(counter()))).unwrap();
        assert_eq!(report.0.components.len(), 1);
        let value: Value = serde_json::from_str(&rendered(Format::Json, &report)).unwrap();
        assert_eq!(value["components"][0]["anchor"], "c.v1.CounterState");
        let text = rendered(Format::Text, &report);
        assert!(
            text.starts_with("CounterState:\n  Kind: aggregate\n"),
            "{text}"
        );
        assert!(text.contains("c.v1.Increment"), "{text}");
        assert!(rendered(Format::Toml, &report).contains("[[components]]"));
    }

    #[test]
    fn an_empty_model_reads_as_none() {
        let report = model(&Fixed(Image::new())).unwrap();
        assert_eq!(rendered(Format::Text, &report), "(none)\n");
        assert_eq!(
            rendered(Format::Json, &report),
            "{\n  \"components\": []\n}\n"
        );
    }

    #[test]
    fn a_source_failure_is_the_use_cases_failure() {
        assert_eq!(lint(&Broken).unwrap_err().code(), "t-unreadable");
        assert_eq!(model(&Broken).unwrap_err().code(), "t-unreadable");
    }
}
