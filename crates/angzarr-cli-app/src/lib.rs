//! The angzarr CLI's use cases. Each loads a descriptor set through the
//! [`DescriptorSource`] port, drives one run of the core's annals domain in
//! memory, and returns its result as data for `angzarr-fmt` to write.
//!
//! - [`lint`]: every finding, as a [`LintReport`] (`{"diagnostics": [...]}`).
//! - [`model`]: the component model, as a [`ModelReport`].
//!
//! [`init`] creates a project's `.angzarr/` directory through the
//! [`Project`] port, and returns what it wrote (or, dry, would write) as an
//! [`InitReport`].

use std::fmt;

use angzarr_cli_core::DescriptorSet;
use angzarr_cli_core::lint::{Finding, Severity as LintSeverity};
use angzarr_cli_core::model::ComponentModel;
use angzarr_cli_core::project::{self, Config, Lock, ProjectError, SpecCommit, Target};
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

/// A project's `.angzarr/` directory: its settings read and, for a new
/// project, written.
pub trait Project {
    /// The project directory as messages name it.
    fn dir(&self) -> String;
    /// Whether the project directory is there.
    fn exists(&self) -> Result<bool, Box<dyn Failure>>;
    /// Reads `config.yaml`.
    fn read_config(&self) -> Result<Config, Box<dyn Failure>>;
    /// Reads `lock.yaml`.
    fn read_lock(&self) -> Result<Lock, Box<dyn Failure>>;
    /// Creates the project directory with `config`, `lock` and the
    /// `.gitignore` that keeps the cache out of version control. Refuses
    /// with [`AppError::ProjectExists`] when the directory is there.
    fn create(&self, config: &Config, lock: &Lock) -> Result<(), Box<dyn Failure>>;
}

/// Invalid project settings as a failure: the core's diagnostic, with its
/// own code and remedy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectInvalid(pub ProjectError);

impl fmt::Display for ProjectInvalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for ProjectInvalid {}

impl Diagnostic for ProjectInvalid {
    fn code(&self) -> &'static str {
        self.0.code()
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        self.0.args()
    }
}

impl Failure for ProjectInvalid {
    fn remedy(&self) -> Option<Report> {
        self.0.remedy()
    }
}

impl From<ProjectError> for ProjectInvalid {
    fn from(e: ProjectError) -> ProjectInvalid {
        ProjectInvalid(e)
    }
}

/// A use case could not finish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppError {
    /// The run refused one of its own steps: a bug in the use case.
    RunRefused { reason: Refused },
    /// `init` found the project directory already there.
    ProjectExists { dir: String },
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::RunRefused { reason } => write!(f, "the run refused a step: {reason}"),
            AppError::ProjectExists { dir } => write!(f, "{dir} already exists"),
        }
    }
}

impl std::error::Error for AppError {}

impl Diagnostic for AppError {
    fn code(&self) -> &'static str {
        match self {
            AppError::RunRefused { .. } => "angzarr-cli-app-run-refused",
            AppError::ProjectExists { .. } => "angzarr-cli-app-project-exists",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        match self {
            AppError::RunRefused { reason } => vec![("reason", Arg::nested(reason))],
            AppError::ProjectExists { dir } => vec![("dir", dir.as_str().into())],
        }
    }
}

impl Failure for AppError {
    fn remedy(&self) -> Option<Report> {
        match self {
            AppError::RunRefused { .. } => None,
            AppError::ProjectExists { dir } => Some(
                Report::new(
                    "angzarr-cli-app-fix-project-exists",
                    format!("edit the settings in {dir}, or remove it to start over"),
                )
                .arg("dir", dir.as_str()),
            ),
        }
    }
}

/// One of every diagnostic this crate raises, for the catalog check.
pub fn diagnostic_samples() -> Vec<Box<dyn Diagnostic>> {
    let exists = AppError::ProjectExists { dir: "x".into() };
    let remedy = exists.remedy().expect("an existing project has a remedy");
    vec![
        Box::new(AppError::RunRefused {
            reason: Refused::NoModel,
        }),
        Box::new(exists),
        Box::new(remedy),
    ]
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

/// What `angzarr init` is asked for, as the user typed it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InitRequest {
    /// Each `--target`.
    pub targets: Vec<String>,
    /// `--spec-commit`.
    pub spec_commit: String,
    /// Report what would be written, and write nothing.
    pub dry_run: bool,
}

/// What `init` wrote, or with `dry_run` would write: the files, relative to
/// the project root, and the settings in them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InitReport {
    pub dry_run: bool,
    pub files: Vec<String>,
    pub config: Config,
    pub lock: Lock,
}

impl Viewable for InitReport {}

/// Creates a new project's `.angzarr/`: a config enabling `targets` and a
/// lock on `spec_commit`. Refuses when the directory is there, dry or not.
pub fn init(project: &dyn Project, request: &InitRequest) -> Result<InitReport, Box<dyn Failure>> {
    let targets = request
        .targets
        .iter()
        .map(|t| Target::parse("--target", t))
        .collect::<Result<Vec<_>, _>>()
        .map_err(ProjectInvalid)?;
    let commit =
        SpecCommit::parse("--spec-commit", &request.spec_commit).map_err(ProjectInvalid)?;
    if project.exists()? {
        return Err(AppError::ProjectExists { dir: project.dir() }.into());
    }
    let config = Config::new(targets);
    let lock = Lock::new(commit);
    if !request.dry_run {
        project.create(&config, &lock)?;
    }
    Ok(InitReport {
        dry_run: request.dry_run,
        files: project::files(),
        config,
        lock,
    })
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
    fn a_component_lists_only_what_it_has_with_the_rest_as_none_or_no() {
        use angzarr_cli_core::model::{Command, Component, ConflictMarker, Event, Kind, State};
        let full = Component {
            anchor: "p.v1.Full".into(),
            file: "p.proto".into(),
            kind: Kind::ProcessManager,
            name: "Full".into(),
            domain: Some("d".into()),
            input_domain: Some("in".into()),
            output_domains: vec!["o1".into(), "o2".into()],
            commands: vec![
                Command {
                    type_name: "p.v1.Go".into(),
                    file: "p.proto".into(),
                    emits: vec!["p.v1.Went".into(), "p.v1.Gone".into()],
                },
                Command {
                    type_name: "p.v1.Idle".into(),
                    file: "p.proto".into(),
                    emits: vec![],
                },
            ],
            events: vec![
                Event {
                    type_name: "p.v1.Went".into(),
                    file: "p.proto".into(),
                    domain: Some("d".into()),
                    applies: true,
                },
                Event {
                    type_name: "p.v1.Gone".into(),
                    file: "p.proto".into(),
                    domain: None,
                    applies: false,
                },
            ],
            state: Some(State {
                conflict_resolutions: vec!["strict", "commutative"],
            }),
            conflict_units: vec![
                ConflictMarker {
                    message: "p.v1.Full".into(),
                    field: "a".into(),
                    descend: true,
                    element_key: Some("id".into()),
                    ordered: true,
                    whole_state: true,
                },
                ConflictMarker {
                    message: "p.v1.Full".into(),
                    field: "b".into(),
                    descend: false,
                    element_key: None,
                    ordered: false,
                    whole_state: false,
                },
            ],
        };
        let bare = Component {
            anchor: "p.v1.Bare".into(),
            file: "p.proto".into(),
            kind: Kind::Projector,
            name: "Bare".into(),
            domain: None,
            input_domain: None,
            output_domains: vec![],
            commands: vec![],
            events: vec![],
            state: Some(State {
                conflict_resolutions: vec![],
            }),
            conflict_units: vec![],
        };
        let report = ModelReport(ComponentModel {
            components: vec![full, bare],
        });
        assert_eq!(
            rendered(Format::Text, &report),
            "Full:\n  Kind: process-manager\n  Anchor: p.v1.Full\n  File: p.proto\n  \
             Domain: d\n  Input domain: in\n  Output domains: o1, o2\n  \
             Conflict resolutions: strict, commutative\n  \
             Commands:\n    Type       Emits\n    p.v1.Go    p.v1.Went, p.v1.Gone\n    \
             p.v1.Idle  (none)\n  \
             Events:\n    Type       Domain  Applies\n    p.v1.Went  d       yes\n    \
             p.v1.Gone  (none)  no\n  \
             Conflict units:\n    Message    Field  Unit\n    \
             p.v1.Full  a      descend, element_key=id, ordered, whole_state\n    \
             p.v1.Full  b      field\n\
             Bare:\n  Kind: projector\n  Anchor: p.v1.Bare\n  File: p.proto\n  \
             Conflict resolutions: (none)\n"
        );
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

    const SHA: &str = "b1f00d2710eb7bee60308f6df520893d782012a6";

    /// A project directory in memory, holding the settings as data.
    #[derive(Default)]
    struct Memory {
        created: std::cell::RefCell<Option<(Config, Lock)>>,
    }

    impl Project for Memory {
        fn dir(&self) -> String {
            "/p/.angzarr".into()
        }
        fn exists(&self) -> Result<bool, Box<dyn Failure>> {
            Ok(self.created.borrow().is_some())
        }
        fn read_config(&self) -> Result<Config, Box<dyn Failure>> {
            Ok(self.created.borrow().clone().unwrap().0)
        }
        fn read_lock(&self) -> Result<Lock, Box<dyn Failure>> {
            Ok(self.created.borrow().clone().unwrap().1)
        }
        fn create(&self, config: &Config, lock: &Lock) -> Result<(), Box<dyn Failure>> {
            if self.exists()? {
                return Err(AppError::ProjectExists { dir: self.dir() }.into());
            }
            *self.created.borrow_mut() = Some((config.clone(), lock.clone()));
            Ok(())
        }
    }

    fn request(targets: &[&str], commit: &str, dry_run: bool) -> InitRequest {
        InitRequest {
            targets: targets.iter().map(|t| t.to_string()).collect(),
            spec_commit: commit.into(),
            dry_run,
        }
    }

    #[test]
    fn init_writes_a_config_with_the_targets_and_a_lock_on_the_commit() {
        let project = Memory::default();
        let report = init(&project, &request(&["python", "rust"], SHA, false)).unwrap();
        assert!(!report.dry_run);
        assert_eq!(
            report.files,
            [
                ".angzarr/config.yaml",
                ".angzarr/lock.yaml",
                ".angzarr/.gitignore"
            ]
        );
        assert_eq!(project.read_config().unwrap(), report.config);
        assert_eq!(project.read_lock().unwrap(), report.lock);
        assert_eq!(report.lock.spec.commit.as_str(), SHA);
        let value: Value = serde_json::from_str(&rendered(Format::Json, &report)).unwrap();
        assert_eq!(
            value["config"],
            json!({
                "protos": "proto",
                "targets": {"rust": {"out": "gen/rust"}, "python": {"out": "gen/python"}},
                "output": {},
            })
        );
        assert_eq!(
            value["lock"],
            json!({"spec": {"commit": SHA}, "targets": {}})
        );
    }

    #[test]
    fn init_with_no_target_enables_none() {
        let project = Memory::default();
        let report = init(&project, &request(&[], SHA, false)).unwrap();
        assert!(report.config.targets.is_empty());
    }

    #[test]
    fn a_dry_init_reports_the_same_and_writes_nothing() {
        let project = Memory::default();
        let dry = init(&project, &request(&["rust"], SHA, true)).unwrap();
        assert!(dry.dry_run);
        assert!(!project.exists().unwrap());
        let wet = init(&project, &request(&["rust"], SHA, false)).unwrap();
        assert_eq!((dry.config, dry.lock), (wet.config, wet.lock));
    }

    #[test]
    fn init_refuses_an_existing_project_with_a_remedy_dry_or_not() {
        let project = Memory::default();
        init(&project, &request(&[], SHA, false)).unwrap();
        for dry_run in [false, true] {
            let err = init(&project, &request(&[], SHA, dry_run)).unwrap_err();
            assert_eq!(err.code(), "angzarr-cli-app-project-exists");
            let remedy = err.remedy().unwrap();
            assert_eq!(remedy.code(), "angzarr-cli-app-fix-project-exists");
            assert_eq!(
                catalog().render(&remedy),
                "edit the settings in /p/.angzarr, or remove it to start over"
            );
        }
    }

    #[test]
    fn init_refuses_an_unknown_target_or_a_short_commit_before_writing() {
        let project = Memory::default();
        let err = init(&project, &request(&["rust", "go"], SHA, false)).unwrap_err();
        assert_eq!(err.code(), "angzarr-cli-core-project-unknown-target");
        assert_eq!(
            catalog().render(&*err),
            "--target: \"go\" is not a target angzarr generates for"
        );
        assert_eq!(
            err.to_string(),
            "--target: \"go\" is not a target angzarr generates for"
        );
        let args: Vec<_> = err
            .args()
            .into_iter()
            .map(|(n, a)| (n, format!("{a:?}")))
            .collect();
        assert_eq!(
            args,
            vec![
                ("source", format!("{:?}", Arg::from("--target"))),
                ("target", format!("{:?}", Arg::from("go"))),
            ]
        );
        assert_eq!(err.remedy().unwrap().code(), "angzarr-cli-core-fix-target");
        let err = init(&project, &request(&[], "b1f00d2", false)).unwrap_err();
        assert_eq!(err.code(), "angzarr-cli-core-project-bad-commit");
        assert!(!project.exists().unwrap());
    }

    #[test]
    fn a_source_failure_is_the_use_cases_failure() {
        assert_eq!(lint(&Broken).unwrap_err().code(), "t-unreadable");
        assert_eq!(model(&Broken).unwrap_err().code(), "t-unreadable");
    }
}
