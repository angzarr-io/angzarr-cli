//! One CLI invocation as an annals domain: its commands are decided against
//! the state folded from its events, with the descriptor set as the decide
//! context. Apply never sees the descriptor set; everything a later step
//! needs from it is recorded in an event.
//!
//! The stream lives in memory for one invocation; [`RunReport`] is its
//! read model.

use std::fmt;

use annals::{Domain, Envelope, Header, Projection, StreamId, Tx};
use annals_diagnostic::{Arg, Diagnostic};

use crate::DescriptorSet;
use crate::lint::{self, Finding};
use crate::model::{self, ComponentModel};

/// The run domain.
pub struct Run;

/// What a run has done so far.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunState {
    pub model: Option<ComponentModel>,
    pub linted: bool,
}

/// What a run is asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunCommand {
    /// Read the component model from the descriptor set.
    ObserveModel,
    /// Check the observed model and the descriptor set.
    Lint,
}

/// What a run records.
#[derive(Clone, Debug, PartialEq)]
pub enum RunEvent {
    ModelObserved(ComponentModel),
    DiagnosticRaised(Finding),
    /// The lint finished, having raised `findings` diagnostics.
    Linted {
        findings: u64,
    },
}

/// Why a run refused a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The model is observed once per run.
    ModelAlreadyObserved,
    /// A lint needs the model observed first.
    NoModel,
    /// A run lints once.
    AlreadyLinted,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Refused::ModelAlreadyObserved => "the model was already observed in this run",
            Refused::NoModel => "the model must be observed before the lint",
            Refused::AlreadyLinted => "this run was already linted",
        })
    }
}

impl Diagnostic for Refused {
    fn code(&self) -> &'static str {
        match self {
            Refused::ModelAlreadyObserved => "angzarr-cli-core-run-model-observed",
            Refused::NoModel => "angzarr-cli-core-run-no-model",
            Refused::AlreadyLinted => "angzarr-cli-core-run-linted",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        Vec::new()
    }
}

impl Domain for Run {
    type State = RunState;
    type Command = RunCommand;
    type Event = RunEvent;
    type Rejection = Refused;
    type Rules = ();
    type Ctx = DescriptorSet;

    fn decide(cmd: &RunCommand, tx: &mut Tx<Self>) -> Result<(), Refused> {
        match cmd {
            RunCommand::ObserveModel => {
                if tx.state().model.is_some() {
                    return Err(Refused::ModelAlreadyObserved);
                }
                let observed = model::build(tx.ctx());
                tx.emit(RunEvent::ModelObserved(observed));
            }
            RunCommand::Lint => {
                let (state, _, set) = tx.parts();
                let Some(observed) = &state.model else {
                    return Err(Refused::NoModel);
                };
                if state.linted {
                    return Err(Refused::AlreadyLinted);
                }
                let findings = lint::run(observed, set);
                let count = findings.len() as u64;
                for finding in findings {
                    tx.emit(RunEvent::DiagnosticRaised(finding));
                }
                tx.emit(RunEvent::Linted { findings: count });
            }
        }
        Ok(())
    }

    fn apply(state: &mut RunState, event: &RunEvent, _: &()) {
        match event {
            RunEvent::ModelObserved(m) => state.model = Some(m.clone()),
            RunEvent::DiagnosticRaised(_) => {}
            RunEvent::Linted { .. } => state.linted = true,
        }
    }
}

/// The header of a run's in-memory stream.
pub fn header() -> Header {
    Header {
        id: StreamId(0),
        domain: "angzarr-cli-run".to_string(),
        schema: 1,
        rules: String::new(),
    }
}

/// What a run produced: the model it observed and the diagnostics it
/// raised, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunReport {
    pub model: Option<ComponentModel>,
    pub findings: Vec<Finding>,
}

impl Projection<RunEvent> for RunReport {
    fn on(&mut self, envelope: &Envelope<RunEvent>) {
        match &envelope.event {
            RunEvent::ModelObserved(m) => self.model = Some(m.clone()),
            RunEvent::DiagnosticRaised(f) => self.findings.push(f.clone()),
            RunEvent::Linted { .. } => {}
        }
    }
}

/// One of every refusal, for the catalog check.
pub fn samples() -> Vec<Box<dyn Diagnostic>> {
    vec![
        Box::new(Refused::ModelAlreadyObserved),
        Box::new(Refused::NoModel),
        Box::new(Refused::AlreadyLinted),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::*;
    use annals::{Stream, project};

    fn given(events: Vec<RunEvent>) -> Stream<Run> {
        let log = events
            .into_iter()
            .enumerate()
            .map(|(i, event)| Envelope {
                seq: i as u64,
                cause: None,
                event,
            })
            .collect();
        Stream::replay(header(), log, &()).unwrap()
    }

    fn when(
        stream: &mut Stream<Run>,
        cmd: RunCommand,
        set: &DescriptorSet,
    ) -> Result<Vec<RunEvent>, Refused> {
        let mut ctx = set.clone();
        stream
            .execute(&cmd, &(), &mut ctx)
            .map(|events| events.iter().map(|e| e.event.clone()).collect())
    }

    fn bad_set() -> DescriptorSet {
        let file = File::new("k.proto", "k.v1").message(Msg::new("A").component(component(
            ComponentKind::Aggregate,
            "",
            "",
            &[],
        )));
        Image::new().file(file).set()
    }

    #[test]
    fn observing_records_the_model() {
        let set = Image::new().file(counter()).set();
        let events = when(&mut given(vec![]), RunCommand::ObserveModel, &set).unwrap();
        assert_eq!(events, vec![RunEvent::ModelObserved(model::build(&set))]);
    }

    #[test]
    fn the_model_is_observed_once() {
        let set = Image::new().file(counter()).set();
        let mut stream = given(vec![RunEvent::ModelObserved(model::build(&set))]);
        assert_eq!(
            when(&mut stream, RunCommand::ObserveModel, &set),
            Err(Refused::ModelAlreadyObserved)
        );
    }

    #[test]
    fn a_lint_needs_the_model() {
        let set = bad_set();
        assert_eq!(
            when(&mut given(vec![]), RunCommand::Lint, &set),
            Err(Refused::NoModel)
        );
    }

    #[test]
    fn a_lint_raises_each_finding_then_records_that_it_ran() {
        let set = bad_set();
        let observed = model::build(&set);
        let mut stream = given(vec![RunEvent::ModelObserved(observed.clone())]);
        let events = when(&mut stream, RunCommand::Lint, &set).unwrap();
        let expected: Vec<RunEvent> = lint::run(&observed, &set)
            .into_iter()
            .map(RunEvent::DiagnosticRaised)
            .chain([RunEvent::Linted { findings: 1 }])
            .collect();
        assert_eq!(events, expected);
        assert_eq!(
            when(&mut stream, RunCommand::Lint, &set),
            Err(Refused::AlreadyLinted)
        );
    }

    #[test]
    fn a_clean_lint_still_records_that_it_ran() {
        let set = Image::new().file(counter()).set();
        let mut stream = given(vec![RunEvent::ModelObserved(model::build(&set))]);
        assert_eq!(
            when(&mut stream, RunCommand::Lint, &set).unwrap(),
            vec![RunEvent::Linted { findings: 0 }]
        );
        assert!(stream.state().linted);
    }

    #[test]
    fn the_report_projects_the_model_and_the_findings() {
        let set = bad_set();
        let mut stream = given(vec![]);
        let mut ctx = set.clone();
        stream
            .execute(&RunCommand::ObserveModel, &(), &mut ctx)
            .unwrap();
        stream.execute(&RunCommand::Lint, &(), &mut ctx).unwrap();
        let mut report = RunReport::default();
        project(&mut report, stream.events());
        assert_eq!(report.model, Some(model::build(&set)));
        assert_eq!(report.findings, lint::run(&model::build(&set), &set));
    }
}
