//! `angzarr probe <component>`: one component of the host config taken
//! through the child protocol by angzarr-embedded-host's [`probe`], a step
//! at a time, and each step's outcome shown. A failed step is its code and
//! args, and its English, rendered here, where the user reads it, through
//! the catalog its code is in ([`catalog::render`]): the spec's for a
//! framework key, the host's for the host's own. A probe with a failed
//! step fails the command, after its steps are shown ([`verdict`]).
//!
//! [`probe`]: angzarr_embedded_host::probe
//! [`catalog::render`]: angzarr_diagnostic::catalog::render

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use angzarr_diagnostic::catalog::{self, Catalog};
use angzarr_diagnostic::{Arg, Diagnostic, Report};
use angzarr_embedded_host::{
    ComponentConfig, ComponentKindConfig, HostConfig, Input, Language, Languages, Outcome, Probed,
    Step,
};
use angzarr_fmt::{Block, Doc, Failure, Viewable};
use clap::Args;
use serde::Serialize;

use crate::CliError;

#[derive(Args)]
pub(crate) struct ProbeArgs {
    /// The component to probe: its name in the host config.
    component: String,
    /// The host config.
    #[arg(long, default_value = "angzarr.yaml")]
    config: PathBuf,
    /// The command a domain is handed: its payload's type URL.
    #[arg(long, value_name = "TYPE_URL", conflicts_with = "event")]
    command: Option<String>,
    /// The event a policy or a projector is handed: its payload's type
    /// URL.
    #[arg(long, value_name = "TYPE_URL")]
    event: Option<String>,
    /// The input's domain, on its cover; the component's name when unset.
    #[arg(long)]
    domain: Option<String>,
    /// A file holding the input's payload, its message encoded; the
    /// message empty (every field its default) when unset.
    #[arg(long, value_name = "FILE")]
    payload: Option<PathBuf>,
}

/// `angzarr probe`: the component's steps, as the probe took them.
pub(crate) fn run(args: ProbeArgs) -> Result<ProbeReport, Box<dyn Failure>> {
    let config = angzarr_embedded_host::config::read(&args.config).map_err(CliError::Host)?;
    let component = listed(&config, &args)?;
    let input = input(&args, component)?;
    let probed =
        angzarr_embedded_host::probe(component, &languages(component), &config.supervision, input);
    Ok(ProbeReport::of(&component.name, &probed))
}

/// The probe's failure, when a step failed: the command fails, naming the
/// first.
pub(crate) fn verdict(report: &ProbeReport) -> Option<Box<dyn Failure>> {
    let failed = report.steps.iter().find(|s| s.outcome == FAILED)?;
    Some(Box::new(CliError::ProbeFailed {
        component: report.component.clone(),
        step: failed.step.to_string(),
    }))
}

/// The `component` entry `args` names.
fn listed<'a>(config: &'a HostConfig, args: &ProbeArgs) -> Result<&'a ComponentConfig, CliError> {
    (config.components.iter())
        .find(|c| c.name == args.component)
        .ok_or_else(|| CliError::ProbeNotListed {
            component: args.component.clone(),
            config: args.config.display().to_string(),
        })
}

/// What `component` is handed: the command or event `args` names, of the
/// domain it names or the component's own, its payload the file it names.
/// An upcaster sidecar is handed nothing, so it needs none.
fn input(args: &ProbeArgs, component: &ComponentConfig) -> Result<Input, CliError> {
    let payload = match &args.payload {
        Some(path) => std::fs::read(path).map_err(|e| CliError::ProbePayloadUnreadable {
            path: path.display().to_string(),
            kind: format!("{:?}", e.kind()),
        })?,
        None => Vec::new(),
    };
    let domain = args.domain.as_deref().unwrap_or(&component.name);
    match (&args.command, &args.event) {
        (Some(type_url), _) => Ok(Input::command(domain, type_url, payload)),
        (None, Some(type_url)) => Ok(Input::event(domain, type_url, payload)),
        (None, None) if component.kind == ComponentKindConfig::Upcaster => {
            Ok(Input::event(domain, "", payload))
        }
        (None, None) => Err(CliError::ProbeNoInput {
            component: component.name.clone(),
            kind: component.kind.as_str().to_string(),
        }),
    }
}

/// The languages the probe launches `component` by: the built-in ones,
/// and the component's own when it is another, a child launched by its
/// `launch` (the cli loads no application, so knows no other language's
/// default).
fn languages(component: &ComponentConfig) -> Languages {
    let builtin = Languages::builtin();
    match builtin.get(&component.language) {
        Some(_) => builtin,
        None => builtin.with(Language::child(component.language.clone())),
    }
}

/// An outcome, as the report names it.
const PASSED: &str = "passed";
const FAILED: &str = "failed";
const NOT_RUN: &str = "not-run";

/// What `angzarr probe` shows: the component, whether it passed, and each
/// step, in order.
#[derive(Serialize)]
pub(crate) struct ProbeReport {
    component: String,
    passed: bool,
    steps: Vec<StepView>,
}

#[derive(Serialize)]
struct StepView {
    step: &'static str,
    /// `passed`, `failed` or `not-run`.
    outcome: &'static str,
    /// Why it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<FailureView>,
}

/// A failure as data: its code, its English, and its args, a nested cause
/// as a failure of the same shape.
#[derive(Serialize)]
struct FailureView {
    code: String,
    message: String,
    args: BTreeMap<String, ArgView>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ArgView {
    Int(i64),
    Uint(u64),
    Str(String),
    Nested(Box<FailureView>),
}

impl ProbeReport {
    fn of(component: &str, probed: &Probed) -> ProbeReport {
        let steps = (probed.steps.iter())
            .map(|(step, outcome)| {
                let (outcome, failure) = match outcome {
                    Outcome::Passed => (PASSED, None),
                    Outcome::Failed(report) => (FAILED, Some(FailureView::of(report))),
                    Outcome::NotRun => (NOT_RUN, None),
                };
                StepView {
                    step: step_name(*step),
                    outcome,
                    failure,
                }
            })
            .collect();
        ProbeReport {
            component: component.to_string(),
            passed: probed.passed(),
            steps,
        }
    }
}

/// A step, as the report names it.
fn step_name(step: Step) -> &'static str {
    match step {
        Step::Launch => "launch",
        Step::Serving => "serving",
        Step::Describe => "describe",
        Step::Handle => "handle",
        Step::Shutdown => "shutdown",
    }
}

/// The text view: the component, whether it passed, and a table of the
/// steps, a failure's detail its English and its code.
impl Viewable for ProbeReport {
    fn view(&self, _: Doc) -> Doc {
        let rows = (self.steps.iter())
            .map(|s| {
                let detail = (s.failure.as_ref())
                    .map(|f| format!("{} [{}]", f.message, f.code))
                    .unwrap_or_default();
                vec![s.step.to_string(), s.outcome.to_string(), detail]
            })
            .collect();
        vec![
            Block::Field {
                label: "Component".into(),
                value: self.component.clone(),
            },
            Block::Field {
                label: "Passed".into(),
                value: self.passed.to_string(),
            },
            Block::Table {
                title: None,
                columns: vec!["Step".into(), "Outcome".into(), "Detail".into()],
                rows,
            },
        ]
    }
}

impl FailureView {
    fn of(d: &dyn Diagnostic) -> FailureView {
        let args = (d.args().into_iter())
            .map(|(name, arg)| {
                let arg = match arg {
                    Arg::Int(n) => ArgView::Int(n),
                    Arg::Uint(n) => ArgView::Uint(n),
                    Arg::Str(s) => ArgView::Str(s),
                    Arg::Nested(cause) => ArgView::Nested(Box::new(FailureView::of(&*cause))),
                };
                (name.to_string(), arg)
            })
            .collect();
        FailureView {
            code: d.code().to_string(),
            message: English(d).to_string(),
            args,
        }
    }
}

/// The catalogs a probe's failures are in: the spec's, for a framework
/// key (core's, and the local-process transport's for a child's; decision
/// 14.64), and the host's, for the host's own codes.
const CATALOGS: [&str; 3] = [
    angzarr_proto::REASONS_EN_US.1,
    angzarr_proto_transport_embedded::REASONS_EN_US.1,
    angzarr_embedded_host::EN_US.1,
];

/// A diagnostic in English: its message in the catalog its code is in,
/// each nested cause first rendered so in its own ([`catalog::render`]);
/// a code in none is shown as its code and args.
struct English<'a>(&'a dyn Diagnostic);

impl fmt::Display for English<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.0;
        let flat = (d.args().into_iter()).fold(Report::new(d.code().to_string()), |r, (n, arg)| {
            let arg = match arg {
                Arg::Nested(cause) => Arg::Str(English(&*cause).to_string()),
                arg => arg,
            };
            r.arg(n.to_string(), arg)
        });
        let ftl = CATALOGS
            .into_iter()
            .find(|ftl| Catalog::parse(ftl).is_ok_and(|c| c.ids().any(|id| id == d.code())));
        match ftl {
            Some(ftl) => catalog::render(ftl, &flat, f),
            None => angzarr_diagnostic::display(d, f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_diagnostic::code;
    use angzarr_embedded_host::HostError;

    /// A probe that took its first `passed` steps, failed the next with
    /// `failure`, and ran none after.
    fn probed(passed: usize, failure: Report) -> Probed {
        let steps = [
            Step::Launch,
            Step::Serving,
            Step::Describe,
            Step::Handle,
            Step::Shutdown,
        ];
        let mut failure = Some(failure);
        Probed {
            steps: (steps.into_iter().enumerate())
                .map(|(n, step)| match n {
                    n if n < passed => (step, Outcome::Passed),
                    n if n == passed => match failure.take() {
                        Some(report) => (step, Outcome::Failed(report)),
                        None => (step, Outcome::Passed),
                    },
                    _ => (step, Outcome::NotRun),
                })
                .collect(),
        }
    }

    /// The host's own error for a child not serving within the limit, so
    /// no code of the host's is spelled here.
    fn timed_out() -> Report {
        Report::of(&HostError::NotServing {
            component: "player".to_string(),
            status: "none".to_string(),
            timeout_ms: 500,
        })
    }

    /// A failed step is its code, its args (a cause nested, of the same
    /// shape) and its English, the cause's rendered in its own catalog.
    #[test]
    fn a_failed_step_is_its_code_args_and_english() {
        let report = ProbeReport::of("player", &probed(1, timed_out()));
        let json = serde_json::to_value(&report).unwrap();
        let serving = timed_out();
        assert_eq!(json["component"], "player");
        assert_eq!(json["passed"], false);
        assert_eq!(
            json["steps"][0],
            serde_json::json!({"step": "launch", "outcome": "passed"})
        );
        assert_eq!(
            json["steps"][1],
            serde_json::json!({
                "step": "serving",
                "outcome": "failed",
                "failure": {
                    "code": serving.code(),
                    "message": "component \"player\" did not report its health SERVING within 500 ms (last none)",
                    "args": {
                        "component": "player",
                        "status": "none",
                        "timeout_ms": 500,
                    },
                },
            })
        );
        assert_eq!(
            json["steps"][2],
            serde_json::json!({"step": "describe", "outcome": "not-run"})
        );
    }

    /// A code in neither catalog (a child's own refusal key) is shown as
    /// its code and args, never guessed.
    #[test]
    fn a_code_in_no_catalog_is_its_code_and_args() {
        let refused = Report::new("TABLE_FULL").arg("seats", 6i64);
        assert_eq!(English(&refused).to_string(), "TABLE_FULL seats=6");
    }

    /// A probe with a failed step fails the command, naming the first; one
    /// that passed every step does not.
    #[test]
    fn a_failed_step_fails_the_command() {
        let failed = ProbeReport::of("player", &probed(3, timed_out()));
        let failure = verdict(&failed).expect("a failure");
        assert_eq!(
            failure.to_string(),
            concat!(code!("probe-failed"), " component=player step=handle")
        );
        let passed = ProbeReport::of("player", &probed(5, timed_out()));
        assert!(verdict(&passed).is_none());
    }

    /// The text view: the component, whether it passed, and the steps, a
    /// failure's English with its code.
    #[test]
    fn the_text_view_is_a_table_of_the_steps() {
        let report = ProbeReport::of("player", &probed(1, timed_out()));
        let Block::Table { rows, columns, .. } = &report.view(Vec::new())[2] else {
            panic!("a table");
        };
        assert_eq!(columns, &["Step", "Outcome", "Detail"]);
        assert_eq!(
            rows[1],
            [
                "serving",
                "failed",
                format!(
                    "component \"player\" did not report its health SERVING within 500 ms (last none) [{}]",
                    timed_out().code()
                )
                .as_str()
            ]
        );
        assert_eq!(rows[0], ["launch", "passed", ""]);
        assert_eq!(rows[4], ["shutdown", "not-run", ""]);
    }

    fn args(argv: &[&str]) -> ProbeArgs {
        #[derive(clap::Parser)]
        struct Probe {
            #[command(flatten)]
            args: ProbeArgs,
        }
        let argv = std::iter::once("probe").chain(argv.iter().copied());
        <Probe as clap::Parser>::parse_from(argv).args
    }

    fn component(kind: ComponentKindConfig, language: &str) -> ComponentConfig {
        ComponentConfig {
            name: "player".to_string(),
            language: language.to_string(),
            kind,
            upcasts: None,
            launch: None,
            entry: Vec::new(),
            dir: None,
            env: Default::default(),
            expose: Default::default(),
        }
    }

    fn cover_domain(input: &Input) -> String {
        match input {
            Input::Command(c) => c
                .input
                .as_ref()
                .unwrap()
                .cover
                .as_ref()
                .unwrap()
                .domain
                .clone(),
            Input::Events(b) => b.cover.as_ref().unwrap().domain.clone(),
        }
    }

    /// The input is the command or event named, of the domain named, else
    /// the component's; an upcaster needs none, any other kind one.
    #[test]
    fn the_input_is_the_one_named() {
        let domain = component(ComponentKindConfig::Domain, "python");
        let given = input(&args(&["player", "--command", "/x.SitDown"]), &domain).unwrap();
        assert!(matches!(given, Input::Command(_)));
        assert_eq!(cover_domain(&given), "player");
        let given = input(
            &args(&["player", "--event", "/x.Seated", "--domain", "table"]),
            &domain,
        )
        .unwrap();
        assert!(matches!(given, Input::Events(_)));
        assert_eq!(cover_domain(&given), "table");
        assert!(matches!(
            input(&args(&["player"]), &domain),
            Err(CliError::ProbeNoInput { component, kind }) if component == "player" && kind == "domain"
        ));
        let upcaster = component(ComponentKindConfig::Upcaster, "python");
        assert!(input(&args(&["player"]), &upcaster).is_ok());
        assert!(matches!(
            input(
                &args(&["player", "--command", "/x", "--payload", "/nonexistent/payload"]),
                &domain
            ),
            Err(CliError::ProbePayloadUnreadable { kind, .. }) if kind == "NotFound"
        ));
    }

    /// A payload file is the input's message, as it is.
    #[test]
    fn a_payload_file_is_the_inputs_message() {
        let dir = std::env::temp_dir().join(format!(
            "{}-payload-{}",
            env!("CARGO_PKG_NAME"),
            std::process::id()
        ));
        std::fs::write(&dir, [8u8, 3]).unwrap();
        let path = dir.display().to_string();
        let domain = component(ComponentKindConfig::Domain, "python");
        let given = input(
            &args(&["player", "--command", "/x", "--payload", &path]),
            &domain,
        );
        let _ = std::fs::remove_file(&dir);
        let Ok(Input::Command(command)) = given else {
            panic!("a command");
        };
        let page = command.input.unwrap().page.unwrap();
        let Some(angzarr_proto::v1::command_page::Body::Command(c)) = page.body else {
            panic!("a command page");
        };
        assert_eq!(c.payload.unwrap().value, [8, 3]);
    }

    /// A component in a language the cli does not build in is launched as
    /// a child, by its `launch`.
    #[test]
    fn another_language_is_a_child() {
        let go = component(ComponentKindConfig::Domain, "go");
        assert!(languages(&go).get("go").is_some());
        assert!(languages(&go).get("python").is_some());
        let python = component(ComponentKindConfig::Domain, "python");
        assert!(languages(&python).get("go").is_none());
    }
}
