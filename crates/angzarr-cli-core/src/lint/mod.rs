//! The lint: every problem with a descriptor set's angzarr declarations,
//! collected at once so a proto is fixed in one round trip.
//!
//! Two families of rules: the wiring rules ([`wiring`]) check that
//! references resolve to the right kind of component, that each kind sets
//! the fields it needs and none it forbids, and that generated names do not
//! collide; the conflict rules ([`conflict`]) check `(state)` and the
//! `(conflict)` markers as options.proto states them.

mod conflict;
mod wiring;

use std::fmt;

use annals_diagnostic::{Arg, Diagnostic, Report};
use prost_reflect::{FieldDescriptor, MessageDescriptor};

use crate::DescriptorSet;
use crate::model::ComponentModel;

/// How much a finding matters: an error makes the declarations unusable,
/// a warning is suspect but works.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// Where a finding is: a proto file, a message, and a field of it when the
/// finding is about one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub file: String,
    /// The message's fully-qualified name.
    pub message: String,
    pub field: Option<String>,
}

impl Location {
    pub fn of_message(m: &MessageDescriptor) -> Location {
        Location {
            file: m.parent_file().name().to_string(),
            message: m.full_name().to_string(),
            field: None,
        }
    }

    pub fn of_field(f: &FieldDescriptor) -> Location {
        Location {
            field: Some(f.name().to_string()),
            ..Location::of_message(f.parent_message())
        }
    }
}

/// `file#message`, or `file#message.field`.
impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.file, self.message)?;
        if let Some(field) = &self.field {
            write!(f, ".{field}")?;
        }
        Ok(())
    }
}

/// One problem found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub location: Location,
    pub lint: Lint,
}

impl Finding {
    pub fn severity(&self) -> Severity {
        self.lint.severity()
    }
}

/// Which `(component)` field a kind requires or forbids.
pub type FieldName = &'static str;

/// Every lint, with its data. Names are fully-qualified proto names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lint {
    /// `(command).component` or `(event).component` names no component in
    /// the set. `marker` is `command` or `event`.
    UnknownComponent {
        marker: &'static str,
        message: String,
        component: String,
    },
    /// `(command).component` names a component that is not an aggregate.
    CommandNotAggregate {
        message: String,
        component: String,
        kind: &'static str,
    },
    /// A `(command).emits` entry is not a message in the set.
    EmitsUnknown { message: String, emitted: String },
    /// A process-manager trigger (`applies` false) without `(event).domain`.
    TriggerWithoutDomain { message: String, component: String },
    /// `(component).kind` is unset or unknown.
    UnspecifiedKind { component: String },
    /// The kind requires `field` and it is empty.
    MissingField {
        component: String,
        kind: &'static str,
        field: FieldName,
    },
    /// The kind forbids `field` and it is set to `value`.
    ForbiddenField {
        component: String,
        kind: &'static str,
        field: FieldName,
        value: String,
    },
    /// A message or field carries angzarr option `number`, but no file in
    /// the set defines that extension.
    OptionUndefined { message: String, number: u32 },
    /// Angzarr option `number` is present but its bytes do not decode.
    MalformedOption { message: String, number: u32 },
    /// Two components generate the same base name.
    NameCollision {
        component: String,
        name: String,
        other: String,
    },
    /// One component generates two methods that render the same in
    /// `language` (`every-language`, `java-typescript` or `python`).
    DuplicateMethod {
        component: String,
        method: String,
        language: &'static str,
        first: String,
        second: String,
    },
    /// A generated top-level name equals a message or enum name in the
    /// anchor's package.
    TypeCollision {
        component: String,
        generated: String,
        package: String,
    },
    /// A command or event generated in this run belongs to a component
    /// whose anchor file is not generated in it.
    SplitRun {
        message: String,
        component: String,
        anchor_file: String,
    },
    /// `(state)` on a message that is not the anchor of an aggregate or a
    /// process manager. `kind` is the anchor's kind, or `none`.
    StateNotOnState { message: String, kind: &'static str },
    /// `(state)` lists CONFLICT_RESOLUTION_UNSPECIFIED.
    UnspecifiedResolution { message: String },
    /// A state lists COMMUTATIVE but marks none of its fields.
    NoMarkedFields { message: String },
    /// A field (or oneof) of a COMMUTATIVE state, or of a message it
    /// descends into, has no `(conflict)`: it conflicts with everything.
    UnmarkedField { message: String, field: String },
    /// `(conflict)` on a field of a message that is neither a COMMUTATIVE
    /// state nor reached from one by descend.
    MarkerOutsideState { message: String, field: String },
    /// `descend` where it cannot apply.
    DescendNonMessage { message: String, field: String },
    /// `element_key` where it cannot apply, or naming no usable field.
    BadElementKey {
        message: String,
        field: String,
        key: String,
    },
    /// `ordered` without `element_key`.
    OrderedWithoutKey { message: String, field: String },
    /// `whole_state` with `descend`, `element_key` or `ordered`.
    WholeStateCombined { message: String, field: String },
    /// A oneof member refines its marker, or only some carry `whole_state`.
    OneofMemberRefined { message: String, oneof: String },
    /// Some but not all members of a oneof carry `(conflict)`.
    OneofPartiallyMarked { message: String, oneof: String },
}

impl Lint {
    pub fn severity(&self) -> Severity {
        match self {
            Lint::UnmarkedField { .. } => Severity::Warning,
            _ => Severity::Error,
        }
    }

    /// What to do about it, when there is something to say.
    pub fn remedy(&self) -> Option<Report> {
        let r = Report::new;
        Some(match self {
            Lint::UnknownComponent { .. } | Lint::SplitRun { .. } => r(
                "angzarr-cli-core-fix-one-run",
                "declare the component, and build one image from every file that declares part of it",
            ),
            Lint::CommandNotAggregate { .. } => r(
                "angzarr-cli-core-fix-command-target",
                "point (command).component at an aggregate's anchor message",
            ),
            Lint::EmitsUnknown { .. } => r(
                "angzarr-cli-core-fix-qualify",
                "name each emitted event by the fully-qualified name of a message in the set",
            ),
            Lint::TriggerWithoutDomain { .. } => r(
                "angzarr-cli-core-fix-trigger-domain",
                "set (event).domain to the domain the trigger comes from, or set applies to fold the event into the process manager's own state",
            ),
            Lint::UnspecifiedKind { .. } => r(
                "angzarr-cli-core-fix-kind",
                "set (component).kind to an aggregate, a saga, a process manager or a projector",
            ),
            Lint::MissingField { field, .. } => Report::new(
                "angzarr-cli-core-fix-set-field",
                format!("set (component).{field}"),
            )
            .arg("field", *field),
            Lint::ForbiddenField { field, .. } => Report::new(
                "angzarr-cli-core-fix-clear-field",
                format!("remove (component).{field}"),
            )
            .arg("field", *field),
            Lint::OptionUndefined { .. } => r(
                "angzarr-cli-core-fix-include-options",
                "build the image with io/angzarr/v1/options.proto and its imports included",
            ),
            Lint::NameCollision { .. } | Lint::TypeCollision { .. } => r(
                "angzarr-cli-core-fix-name",
                "set a (component).name distinct from every other component and from every message and enum in the package",
            ),
            Lint::DuplicateMethod { .. } => r(
                "angzarr-cli-core-fix-rename-message",
                "rename one of the two messages",
            ),
            Lint::StateNotOnState { .. } => r(
                "angzarr-cli-core-fix-move-state",
                "put (state) on the state message of an aggregate or a process manager",
            ),
            Lint::UnspecifiedResolution { .. } => r(
                "angzarr-cli-core-fix-list-resolution",
                "list CONFLICT_RESOLUTION_COMMUTATIVE, or nothing: STRICT and AGGREGATE_HANDLES are always supported",
            ),
            Lint::NoMarkedFields { .. } | Lint::UnmarkedField { .. } => r(
                "angzarr-cli-core-fix-mark-field",
                "mark the field with (conflict), or with (conflict) = {whole_state: true} when it depends on the rest of the state",
            ),
            Lint::MarkerOutsideState { .. } => r(
                "angzarr-cli-core-fix-commutative",
                "list CONFLICT_RESOLUTION_COMMUTATIVE in the state's (state) option and reach this message by descend, or remove the marker",
            ),
            Lint::DescendNonMessage { .. } => r(
                "angzarr-cli-core-fix-descend",
                "remove descend, or give a repeated message field an element_key",
            ),
            Lint::BadElementKey { .. } => r(
                "angzarr-cli-core-fix-element-key",
                "name a singular scalar, string, bytes or enum field of the element message",
            ),
            Lint::OrderedWithoutKey { .. } => r(
                "angzarr-cli-core-fix-ordered",
                "set element_key, or remove ordered",
            ),
            Lint::WholeStateCombined { .. } => r(
                "angzarr-cli-core-fix-whole-state",
                "use whole_state on its own",
            ),
            Lint::OneofMemberRefined { .. } | Lint::OneofPartiallyMarked { .. } => r(
                "angzarr-cli-core-fix-oneof",
                "give every member of the oneof an empty (conflict), or every member whole_state",
            ),
            Lint::MalformedOption { .. } => return None,
        })
    }
}

impl fmt::Display for Lint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lint::UnknownComponent {
                marker,
                message,
                component,
            } => write!(
                f,
                "{message}: ({marker}).component {component} is not a component declared in this set"
            ),
            Lint::CommandNotAggregate {
                message,
                component,
                kind,
            } => write!(
                f,
                "{message}: (command).component {component} is a {kind}; only aggregates handle commands"
            ),
            Lint::EmitsUnknown { message, emitted } => write!(
                f,
                "{message}: (command).emits \"{emitted}\" is not the fully-qualified name of a message in the set"
            ),
            Lint::TriggerWithoutDomain { message, component } => write!(
                f,
                "{message}: a process-manager trigger for {component} needs (event).domain"
            ),
            Lint::UnspecifiedKind { component } => {
                write!(f, "{component}: (component).kind is not set")
            }
            Lint::MissingField {
                component,
                kind,
                field,
            } => write!(f, "{component}: a {kind} requires (component).{field}"),
            Lint::ForbiddenField {
                component,
                kind,
                field,
                value,
            } => write!(
                f,
                "{component}: a {kind} must not set (component).{field} (it is \"{value}\")"
            ),
            Lint::OptionUndefined { message, number } => write!(
                f,
                "{message} carries angzarr option {number}, but no file in the set defines it"
            ),
            Lint::MalformedOption { message, number } => write!(
                f,
                "{message} carries angzarr option {number}, but its bytes do not decode"
            ),
            Lint::NameCollision {
                component,
                name,
                other,
            } => write!(
                f,
                "{component} generates the name {name}, which {other} generates too"
            ),
            Lint::DuplicateMethod {
                component,
                method,
                language,
                first,
                second,
            } => write!(
                f,
                "{component} generates the method {method} twice in {language} (from {first} and {second})"
            ),
            Lint::TypeCollision {
                component,
                generated,
                package,
            } => write!(
                f,
                "{component} generates {generated}, which is already a message or enum in package {package}"
            ),
            Lint::SplitRun {
                message,
                component,
                anchor_file,
            } => write!(
                f,
                "{message} is generated in this run, but its component {component} is declared in {anchor_file}, which is not"
            ),
            Lint::StateNotOnState { message, kind } => write!(
                f,
                "{message} carries (state), but it is not the state of an aggregate or a process manager (it is: {kind})"
            ),
            Lint::UnspecifiedResolution { message } => write!(
                f,
                "{message}: (state) lists CONFLICT_RESOLUTION_UNSPECIFIED"
            ),
            Lint::NoMarkedFields { message } => write!(
                f,
                "{message} lists COMMUTATIVE but marks no field with (conflict)"
            ),
            Lint::UnmarkedField { message, field } => write!(
                f,
                "{message}.{field} has no (conflict) marker, so it conflicts with every change"
            ),
            Lint::MarkerOutsideState { message, field } => write!(
                f,
                "{message}.{field} carries (conflict), but {message} is not a COMMUTATIVE state or reached from one by descend"
            ),
            Lint::DescendNonMessage { message, field } => write!(
                f,
                "{message}.{field}: descend needs a message field, a map of messages, or a repeated message with element_key"
            ),
            Lint::BadElementKey {
                message,
                field,
                key,
            } => write!(
                f,
                "{message}.{field}: element_key \"{key}\" needs a repeated message field whose element has a singular scalar, string, bytes or enum field of that name"
            ),
            Lint::OrderedWithoutKey { message, field } => {
                write!(f, "{message}.{field}: ordered needs element_key")
            }
            Lint::WholeStateCombined { message, field } => write!(
                f,
                "{message}.{field}: whole_state cannot be combined with descend, element_key or ordered"
            ),
            Lint::OneofMemberRefined { message, oneof } => write!(
                f,
                "{message}.{oneof}: oneof members carry only an empty (conflict), or all carry whole_state"
            ),
            Lint::OneofPartiallyMarked { message, oneof } => write!(
                f,
                "{message}.{oneof}: some but not all members of the oneof carry (conflict)"
            ),
        }
    }
}

impl Diagnostic for Lint {
    fn code(&self) -> &'static str {
        match self {
            Lint::UnknownComponent { .. } => "angzarr-cli-core-unknown-component",
            Lint::CommandNotAggregate { .. } => "angzarr-cli-core-command-not-aggregate",
            Lint::EmitsUnknown { .. } => "angzarr-cli-core-emits-unknown",
            Lint::TriggerWithoutDomain { .. } => "angzarr-cli-core-trigger-without-domain",
            Lint::UnspecifiedKind { .. } => "angzarr-cli-core-unspecified-kind",
            Lint::MissingField { .. } => "angzarr-cli-core-missing-field",
            Lint::ForbiddenField { .. } => "angzarr-cli-core-forbidden-field",
            Lint::OptionUndefined { .. } => "angzarr-cli-core-option-undefined",
            Lint::MalformedOption { .. } => "angzarr-cli-core-malformed-option",
            Lint::NameCollision { .. } => "angzarr-cli-core-name-collision",
            Lint::DuplicateMethod { .. } => "angzarr-cli-core-duplicate-method",
            Lint::TypeCollision { .. } => "angzarr-cli-core-type-collision",
            Lint::SplitRun { .. } => "angzarr-cli-core-split-run",
            Lint::StateNotOnState { .. } => "angzarr-cli-core-state-not-on-state",
            Lint::UnspecifiedResolution { .. } => "angzarr-cli-core-unspecified-resolution",
            Lint::NoMarkedFields { .. } => "angzarr-cli-core-no-marked-fields",
            Lint::UnmarkedField { .. } => "angzarr-cli-core-unmarked-field",
            Lint::MarkerOutsideState { .. } => "angzarr-cli-core-marker-outside-state",
            Lint::DescendNonMessage { .. } => "angzarr-cli-core-descend-non-message",
            Lint::BadElementKey { .. } => "angzarr-cli-core-bad-element-key",
            Lint::OrderedWithoutKey { .. } => "angzarr-cli-core-ordered-without-key",
            Lint::WholeStateCombined { .. } => "angzarr-cli-core-whole-state-combined",
            Lint::OneofMemberRefined { .. } => "angzarr-cli-core-oneof-member-refined",
            Lint::OneofPartiallyMarked { .. } => "angzarr-cli-core-oneof-partially-marked",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        let s = |v: &str| Arg::from(v);
        match self {
            Lint::UnknownComponent {
                marker,
                message,
                component,
            } => vec![
                ("marker", s(marker)),
                ("message", s(message)),
                ("component", s(component)),
            ],
            Lint::CommandNotAggregate {
                message,
                component,
                kind,
            } => vec![
                ("message", s(message)),
                ("component", s(component)),
                ("kind", s(kind)),
            ],
            Lint::EmitsUnknown { message, emitted } => {
                vec![("message", s(message)), ("emitted", s(emitted))]
            }
            Lint::TriggerWithoutDomain { message, component } => {
                vec![("message", s(message)), ("component", s(component))]
            }
            Lint::UnspecifiedKind { component } => vec![("component", s(component))],
            Lint::MissingField {
                component,
                kind,
                field,
            } => vec![
                ("component", s(component)),
                ("kind", s(kind)),
                ("field", s(field)),
            ],
            Lint::ForbiddenField {
                component,
                kind,
                field,
                value,
            } => vec![
                ("component", s(component)),
                ("kind", s(kind)),
                ("field", s(field)),
                ("value", s(value)),
            ],
            Lint::OptionUndefined { message, number }
            | Lint::MalformedOption { message, number } => {
                vec![("message", s(message)), ("number", Arg::from(*number))]
            }
            Lint::NameCollision {
                component,
                name,
                other,
            } => vec![
                ("component", s(component)),
                ("name", s(name)),
                ("other", s(other)),
            ],
            Lint::DuplicateMethod {
                component,
                method,
                language,
                first,
                second,
            } => vec![
                ("component", s(component)),
                ("method", s(method)),
                ("language", s(language)),
                ("first", s(first)),
                ("second", s(second)),
            ],
            Lint::TypeCollision {
                component,
                generated,
                package,
            } => vec![
                ("component", s(component)),
                ("generated", s(generated)),
                ("package", s(package)),
            ],
            Lint::SplitRun {
                message,
                component,
                anchor_file,
            } => vec![
                ("message", s(message)),
                ("component", s(component)),
                ("anchor_file", s(anchor_file)),
            ],
            Lint::StateNotOnState { message, kind } => {
                vec![("message", s(message)), ("kind", s(kind))]
            }
            Lint::UnspecifiedResolution { message } | Lint::NoMarkedFields { message } => {
                vec![("message", s(message))]
            }
            Lint::UnmarkedField { message, field }
            | Lint::MarkerOutsideState { message, field }
            | Lint::DescendNonMessage { message, field }
            | Lint::OrderedWithoutKey { message, field }
            | Lint::WholeStateCombined { message, field } => {
                vec![("message", s(message)), ("field", s(field))]
            }
            Lint::BadElementKey {
                message,
                field,
                key,
            } => vec![
                ("message", s(message)),
                ("field", s(field)),
                ("key", s(key)),
            ],
            Lint::OneofMemberRefined { message, oneof }
            | Lint::OneofPartiallyMarked { message, oneof } => {
                vec![("message", s(message)), ("oneof", s(oneof))]
            }
        }
    }
}

/// Every finding for a descriptor set and the model built from it: the
/// wiring rules, then the conflict rules.
pub fn run(model: &ComponentModel, set: &DescriptorSet) -> Vec<Finding> {
    let mut findings = wiring::run(model, set);
    findings.extend(conflict::run(model, set));
    findings
}

/// One of every lint and every remedy, for the catalog check.
pub fn samples() -> Vec<Box<dyn Diagnostic>> {
    let s = || "x".to_string();
    let lints = vec![
        Lint::UnknownComponent {
            marker: "command",
            message: s(),
            component: s(),
        },
        Lint::CommandNotAggregate {
            message: s(),
            component: s(),
            kind: "saga",
        },
        Lint::EmitsUnknown {
            message: s(),
            emitted: s(),
        },
        Lint::TriggerWithoutDomain {
            message: s(),
            component: s(),
        },
        Lint::UnspecifiedKind { component: s() },
        Lint::MissingField {
            component: s(),
            kind: "aggregate",
            field: "domain",
        },
        Lint::ForbiddenField {
            component: s(),
            kind: "saga",
            field: "domain",
            value: s(),
        },
        Lint::OptionUndefined {
            message: s(),
            number: 50100,
        },
        Lint::MalformedOption {
            message: s(),
            number: 50100,
        },
        Lint::NameCollision {
            component: s(),
            name: s(),
            other: s(),
        },
        Lint::DuplicateMethod {
            component: s(),
            method: s(),
            language: "python",
            first: s(),
            second: s(),
        },
        Lint::TypeCollision {
            component: s(),
            generated: s(),
            package: s(),
        },
        Lint::SplitRun {
            message: s(),
            component: s(),
            anchor_file: s(),
        },
        Lint::StateNotOnState {
            message: s(),
            kind: "none",
        },
        Lint::UnspecifiedResolution { message: s() },
        Lint::NoMarkedFields { message: s() },
        Lint::UnmarkedField {
            message: s(),
            field: s(),
        },
        Lint::MarkerOutsideState {
            message: s(),
            field: s(),
        },
        Lint::DescendNonMessage {
            message: s(),
            field: s(),
        },
        Lint::BadElementKey {
            message: s(),
            field: s(),
            key: s(),
        },
        Lint::OrderedWithoutKey {
            message: s(),
            field: s(),
        },
        Lint::WholeStateCombined {
            message: s(),
            field: s(),
        },
        Lint::OneofMemberRefined {
            message: s(),
            oneof: s(),
        },
        Lint::OneofPartiallyMarked {
            message: s(),
            oneof: s(),
        },
    ];
    let mut out: Vec<Box<dyn Diagnostic>> = Vec::new();
    let mut remedies = std::collections::BTreeMap::new();
    for lint in lints {
        if let Some(r) = lint.remedy() {
            remedies.entry(r.code()).or_insert(r);
        }
        out.push(Box::new(lint));
    }
    out.extend(
        remedies
            .into_values()
            .map(|r| Box::new(r) as Box<dyn Diagnostic>),
    );
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The findings with `code`, as (location, lint) pairs.
    pub(crate) fn with_code(findings: &[Finding], code: &str) -> Vec<Finding> {
        findings
            .iter()
            .filter(|f| f.lint.code() == code)
            .cloned()
            .collect()
    }

    #[test]
    fn a_location_names_the_file_message_and_field() {
        let mut l = Location {
            file: "a/b.proto".into(),
            message: "a.v1.M".into(),
            field: None,
        };
        assert_eq!(l.to_string(), "a/b.proto#a.v1.M");
        l.field = Some("f".into());
        assert_eq!(l.to_string(), "a/b.proto#a.v1.M.f");
    }

    #[test]
    fn only_an_unmarked_field_is_a_warning() {
        assert_eq!(
            Lint::UnmarkedField {
                message: "m".into(),
                field: "f".into()
            }
            .severity(),
            Severity::Warning
        );
        assert_eq!(
            Lint::NoMarkedFields {
                message: "m".into()
            }
            .severity(),
            Severity::Error
        );
    }
}
