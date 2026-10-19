//! The component model: every component a descriptor set declares, with
//! the commands and events attached to it.
//!
//! A component is declared by its anchor message carrying `(component)`;
//! its commands and events are other messages pointing back at the anchor
//! by fully-qualified name. A reference that does not resolve to a
//! component of the right kind is left out of the model (the lint reports
//! it), so the model holds only what would be wired.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use angzarr_proto::v1::{ComponentKind, ComponentOptions, ConflictResolution, ConflictUnit};
use prost::Message;
use prost_reflect::{FieldDescriptor, Kind as FieldKind, MessageDescriptor};
use serde::Serialize;

use crate::DescriptorSet;
use crate::options::{FieldOpts, MessageOpts, field_opts, message_opts};

/// How deep `descend` reaches, in descended messages counted from the
/// state message (options.proto, "Recursion bound").
pub const DESCEND_DEPTH: usize = 32;

/// Which dispatch component a declaration generates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Unspecified,
    Aggregate,
    Saga,
    ProcessManager,
    Projector,
}

impl Kind {
    /// The kind a `(component).kind` number names; an unknown number is
    /// unspecified.
    pub fn from_proto(kind: i32) -> Kind {
        match ComponentKind::try_from(kind) {
            Ok(ComponentKind::Aggregate) => Kind::Aggregate,
            Ok(ComponentKind::Saga) => Kind::Saga,
            Ok(ComponentKind::ProcessManager) => Kind::ProcessManager,
            Ok(ComponentKind::Projector) => Kind::Projector,
            _ => Kind::Unspecified,
        }
    }

    /// The kind as a kebab-case token, the same in every format.
    pub fn token(self) -> &'static str {
        match self {
            Kind::Unspecified => "unspecified",
            Kind::Aggregate => "aggregate",
            Kind::Saga => "saga",
            Kind::ProcessManager => "process-manager",
            Kind::Projector => "projector",
        }
    }

    /// Whether the kind has event-sourced state (and so may carry `(state)`).
    pub fn is_stateful(self) -> bool {
        matches!(self, Kind::Aggregate | Kind::ProcessManager)
    }
}

/// A conflict resolution as a kebab-case token.
pub fn resolution_token(value: i32) -> &'static str {
    match ConflictResolution::try_from(value) {
        Ok(ConflictResolution::Commutative) => "commutative",
        Ok(ConflictResolution::Strict) => "strict",
        Ok(ConflictResolution::AggregateHandles) => "aggregate-handles",
        _ => "unspecified",
    }
}

/// Every component in a descriptor set, in declaration order.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ComponentModel {
    pub components: Vec<Component>,
}

/// One declared component.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Component {
    /// The anchor message's fully-qualified name: what commands and events
    /// name as their component.
    pub anchor: String,
    /// The proto file declaring the anchor.
    pub file: String,
    pub kind: Kind,
    /// The generated base name: `(component).name`, or the anchor's name.
    pub name: String,
    /// The stream the component owns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// The domain it subscribes to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_domain: Option<String>,
    /// The domains it sends commands to: `output_domain`, then each of
    /// `output_domains`, without blanks or repeats.
    pub output_domains: Vec<String>,
    /// The commands it handles.
    pub commands: Vec<Command>,
    /// The events it consumes.
    pub events: Vec<Event>,
    /// The anchor's `(state)` option, when it carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<State>,
    /// The `(conflict)` markers on the anchor's fields and on the fields of
    /// every message reached from it by `descend`.
    pub conflict_units: Vec<ConflictMarker>,
}

/// A command a component handles.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Command {
    /// The command message's fully-qualified name.
    #[serde(rename = "type")]
    pub type_name: String,
    pub file: String,
    /// The fully-qualified event types it emits, as declared.
    pub emits: Vec<String>,
}

/// An event a component consumes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Event {
    /// The event message's fully-qualified name.
    #[serde(rename = "type")]
    pub type_name: String,
    pub file: String,
    /// The source domain, when the entry names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Whether the event is folded into the component's state (an applier)
    /// rather than reacted to (a handler): always for an aggregate, never
    /// for a saga or projector, `(event).applies` for a process manager.
    pub applies: bool,
}

/// A state message's `(state)` option.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct State {
    /// The resolutions it lists beyond the implicit STRICT and
    /// AGGREGATE_HANDLES, as tokens.
    pub conflict_resolutions: Vec<&'static str>,
}

/// One `(conflict)` marker.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConflictMarker {
    /// The fully-qualified name of the message declaring the field.
    pub message: String,
    pub field: String,
    pub descend: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_key: Option<String>,
    pub ordered: bool,
    pub whole_state: bool,
}

/// One message and the angzarr options it carries.
#[derive(Clone, Debug)]
pub struct Declared {
    pub message: MessageDescriptor,
    pub opts: MessageOpts,
}

impl Declared {
    pub fn fq(&self) -> &str {
        self.message.full_name()
    }

    pub fn file(&self) -> String {
        self.message.parent_file().name().to_string()
    }
}

/// Every message in the set, files in pool order, each file's messages in
/// declaration order with nested messages after their parent.
pub fn messages(set: &DescriptorSet) -> Vec<MessageDescriptor> {
    fn walk(m: MessageDescriptor, out: &mut Vec<MessageDescriptor>) {
        let nested: Vec<_> = m.child_messages().collect();
        out.push(m);
        for n in nested {
            walk(n, out);
        }
    }
    let mut out = Vec::new();
    for file in set.pool.files() {
        for m in file.messages() {
            walk(m, &mut out);
        }
    }
    out
}

/// Every message carrying at least one angzarr option, in [`messages`]
/// order.
pub fn declarations(set: &DescriptorSet) -> Vec<Declared> {
    messages(set)
        .into_iter()
        .filter_map(|message| {
            let opts = message_opts(&message.options().encode_to_vec());
            (!opts.present.is_empty()).then_some(Declared { message, opts })
        })
        .collect()
}

/// The `(conflict)` option on a field.
pub fn field_marker(field: &FieldDescriptor) -> FieldOpts {
    field_opts(&field.options().encode_to_vec())
}

/// The generated base name of a component anchored at `anchor`.
pub fn base_name(anchor: &MessageDescriptor, c: &ComponentOptions) -> String {
    if c.name.is_empty() {
        anchor.name().to_string()
    } else {
        c.name.clone()
    }
}

fn non_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// `output_domain` then `output_domains`, without blanks or repeats.
pub fn output_domains(c: &ComponentOptions) -> Vec<String> {
    let mut seen = BTreeSet::new();
    std::iter::once(&c.output_domain)
        .chain(&c.output_domains)
        .filter(|d| !d.is_empty() && seen.insert(d.as_str()))
        .cloned()
        .collect()
}

/// The message a descending field splits into: a singular message field's
/// type, a map's message value type, or a repeated message element's type.
pub fn descended_message(field: &FieldDescriptor) -> Option<MessageDescriptor> {
    if field.is_map() {
        let FieldKind::Message(entry) = field.kind() else {
            return None;
        };
        return match entry.map_entry_value_field().kind() {
            FieldKind::Message(value) => Some(value),
            _ => None,
        };
    }
    match field.kind() {
        FieldKind::Message(m) => Some(m),
        _ => None,
    }
}

/// The messages reached from `root` by valid `descend` markers, `root`
/// first, each with its depth, breadth first so each depth is the least.
/// A message at [`DESCEND_DEPTH`] is reached, but its fields do not descend
/// further.
pub fn descended(root: &MessageDescriptor) -> Vec<(MessageDescriptor, usize)> {
    let mut seen = BTreeSet::from([root.full_name().to_string()]);
    let mut out = Vec::new();
    let mut queue = VecDeque::from([(root.clone(), 0usize)]);
    while let Some((m, depth)) = queue.pop_front() {
        if depth < DESCEND_DEPTH {
            for field in m.fields() {
                let unit = field_marker(&field).conflict.unwrap_or_default();
                if unit.descend
                    && descend_valid(&field, &unit)
                    && let Some(inner) = descended_message(&field)
                    && seen.insert(inner.full_name().to_string())
                {
                    queue.push_back((inner, depth + 1));
                }
            }
        }
        out.push((m, depth));
    }
    out
}

/// Whether `descend` is allowed on `field`: a singular message, a map of
/// messages, or a repeated message with `element_key`.
pub fn descend_valid(field: &FieldDescriptor, unit: &ConflictUnit) -> bool {
    if field.is_map() {
        return descended_message(field).is_some();
    }
    match field.kind() {
        FieldKind::Message(_) => !field.is_list() || !unit.element_key.is_empty(),
        _ => false,
    }
}

fn markers(anchor: &MessageDescriptor) -> Vec<ConflictMarker> {
    let mut out = Vec::new();
    for (m, _) in descended(anchor) {
        for field in m.fields() {
            if let Some(unit) = field_marker(&field).conflict {
                out.push(ConflictMarker {
                    message: m.full_name().to_string(),
                    field: field.name().to_string(),
                    descend: unit.descend,
                    element_key: non_empty(&unit.element_key),
                    ordered: unit.ordered,
                    whole_state: unit.whole_state,
                });
            }
        }
    }
    out
}

/// The component model of a descriptor set.
pub fn build(set: &DescriptorSet) -> ComponentModel {
    let declared = declarations(set);
    let mut components = Vec::new();
    let mut index = BTreeMap::new();
    for d in &declared {
        let Some(c) = &d.opts.component else { continue };
        index.insert(d.fq().to_string(), components.len());
        components.push(Component {
            anchor: d.fq().to_string(),
            file: d.file(),
            kind: Kind::from_proto(c.kind),
            name: base_name(&d.message, c),
            domain: non_empty(&c.domain),
            input_domain: non_empty(&c.input_domain),
            output_domains: output_domains(c),
            commands: Vec::new(),
            events: Vec::new(),
            state: d.opts.state.as_ref().map(|s| State {
                conflict_resolutions: s
                    .conflict_resolutions
                    .iter()
                    .map(|r| resolution_token(*r))
                    .collect(),
            }),
            conflict_units: markers(&d.message),
        });
    }
    for d in &declared {
        if let Some(cmd) = &d.opts.command
            && let Some(&i) = index.get(&cmd.component)
            && components[i].kind == Kind::Aggregate
        {
            components[i].commands.push(Command {
                type_name: d.fq().to_string(),
                file: d.file(),
                emits: cmd.emits.clone(),
            });
        }
        for ev in &d.opts.events {
            let Some(&i) = index.get(&ev.component) else {
                continue;
            };
            let applies = match components[i].kind {
                Kind::Aggregate => true,
                Kind::ProcessManager if ev.applies => true,
                Kind::ProcessManager if !ev.domain.is_empty() => false,
                Kind::Saga | Kind::Projector => false,
                _ => continue,
            };
            components[i].events.push(Event {
                type_name: d.fq().to_string(),
                file: d.file(),
                domain: non_empty(&ev.domain),
                applies,
            });
        }
    }
    ComponentModel { components }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::*;

    fn only(model: &ComponentModel) -> &Component {
        assert_eq!(model.components.len(), 1, "{model:?}");
        &model.components[0]
    }

    #[test]
    fn an_aggregate_with_its_command_and_applied_event() {
        let model = build(&Image::new().file(counter()).set());
        let c = only(&model);
        assert_eq!(c.anchor, "c.v1.CounterState");
        assert_eq!(c.file, "counter.proto");
        assert_eq!(c.kind, Kind::Aggregate);
        assert_eq!(c.name, "CounterState");
        assert_eq!(c.domain.as_deref(), Some("counter"));
        assert_eq!(
            c.commands,
            vec![Command {
                type_name: "c.v1.Increment".into(),
                file: "counter.proto".into(),
                emits: vec!["c.v1.Incremented".into()],
            }]
        );
        assert_eq!(
            c.events,
            vec![Event {
                type_name: "c.v1.Incremented".into(),
                file: "counter.proto".into(),
                domain: None,
                applies: true,
            }]
        );
        assert_eq!(c.state, None);
    }

    #[test]
    fn options_survive_a_set_that_does_not_define_them() {
        let model = build(&Image::bare().file(counter()).set());
        assert_eq!(only(&model).commands.len(), 1);
    }

    #[test]
    fn a_saga_reacts_and_takes_its_output_domains_in_order_without_repeats() {
        let mut opts = component(
            ComponentKind::Saga,
            "",
            "orders",
            &["stock", "ship", "stock"],
        );
        opts.output_domain = "ship".into();
        opts.name = "Fulfil".into();
        let file = File::new("saga.proto", "s.v1")
            .message(Msg::new("FulfilSaga").component(opts))
            .message(Msg::new("Placed").event(event("s.v1.FulfilSaga", "orders", false)));
        let c = build(&Image::new().file(file).set()).components.remove(0);
        assert_eq!(c.kind, Kind::Saga);
        assert_eq!(c.name, "Fulfil");
        assert_eq!(c.domain, None);
        assert_eq!(c.input_domain.as_deref(), Some("orders"));
        assert_eq!(c.output_domains, vec!["ship", "stock"]);
        assert_eq!(c.events[0].domain.as_deref(), Some("orders"));
        assert!(!c.events[0].applies);
    }

    #[test]
    fn a_process_manager_folds_its_own_events_and_reacts_to_triggers() {
        let file = File::new("pm.proto", "p.v1")
            .message(Msg::new("FlowState").component(component(
                ComponentKind::ProcessManager,
                "flow",
                "",
                &["stock"],
            )))
            .message(Msg::new("Started").event(event("p.v1.FlowState", "", true)))
            .message(Msg::new("Paid").event(event("p.v1.FlowState", "billing", false)))
            .message(Msg::new("Orphan").event(event("p.v1.FlowState", "", false)));
        let c = build(&Image::new().file(file).set()).components.remove(0);
        assert_eq!(c.kind, Kind::ProcessManager);
        let events: Vec<_> = c
            .events
            .iter()
            .map(|e| (e.type_name.as_str(), e.domain.as_deref(), e.applies))
            .collect();
        assert_eq!(
            events,
            vec![
                ("p.v1.Started", None, true),
                ("p.v1.Paid", Some("billing"), false),
            ]
        );
    }

    #[test]
    fn a_projector_reacts_and_an_event_may_feed_several_components() {
        let file = counter()
            .message(Msg::new("Totals").component(component(
                ComponentKind::Projector,
                "",
                "counter",
                &[],
            )))
            .message(
                Msg::new("Reset")
                    .event(event("c.v1.CounterState", "", false))
                    .event(event("c.v1.Totals", "counter", false)),
            );
        let model = build(&Image::new().file(file).set());
        assert_eq!(model.components.len(), 2);
        let projector = &model.components[1];
        assert_eq!(projector.kind, Kind::Projector);
        assert_eq!(projector.events[0].type_name, "c.v1.Reset");
        assert!(!projector.events[0].applies);
        assert_eq!(model.components[0].events.len(), 2);
    }

    #[test]
    fn unresolved_and_misdirected_references_are_left_out() {
        let file = counter()
            .message(Msg::new("Lost").command(command("c.v1.Nowhere", &[])))
            .message(Msg::new("Totals").component(component(
                ComponentKind::Projector,
                "",
                "counter",
                &[],
            )))
            .message(Msg::new("Misdirected").command(command("c.v1.Totals", &[])));
        let model = build(&Image::new().file(file).set());
        assert_eq!(model.components[0].commands.len(), 1);
        assert!(model.components[1].commands.is_empty());
    }

    #[test]
    fn state_options_and_conflict_markers_through_descend() {
        let file = File::new("list.proto", "l.v1")
            .message(
                Msg::new("ListState")
                    .component(component(ComponentKind::Aggregate, "list", "", &[]))
                    .state(state(&[ConflictResolution::Commutative]))
                    .field(Fld::scalar("title", 1, ty::STRING).conflict(ConflictUnit::default()))
                    .field(
                        Fld::message("settings", 2, ".l.v1.Settings").conflict(ConflictUnit {
                            descend: true,
                            ..Default::default()
                        }),
                    )
                    .field(Fld::scalar("notes", 3, ty::STRING)),
            )
            .message(
                Msg::new("Settings").field(Fld::scalar("days", 1, ty::INT64).conflict(
                    ConflictUnit {
                        whole_state: true,
                        ..Default::default()
                    },
                )),
            );
        let c = build(&Image::new().file(file).set()).components.remove(0);
        assert_eq!(
            c.state,
            Some(State {
                conflict_resolutions: vec!["commutative"]
            })
        );
        let marked: Vec<_> = c
            .conflict_units
            .iter()
            .map(|m| {
                (
                    m.message.as_str(),
                    m.field.as_str(),
                    m.descend,
                    m.whole_state,
                )
            })
            .collect();
        assert_eq!(
            marked,
            vec![
                ("l.v1.ListState", "title", false, false),
                ("l.v1.ListState", "settings", true, false),
                ("l.v1.Settings", "days", false, true),
            ]
        );
    }

    #[test]
    fn every_resolution_has_its_own_token() {
        for (r, token) in [
            (ConflictResolution::Commutative, "commutative"),
            (ConflictResolution::Strict, "strict"),
            (ConflictResolution::AggregateHandles, "aggregate-handles"),
            (ConflictResolution::Unspecified, "unspecified"),
        ] {
            assert_eq!(resolution_token(r as i32), token);
        }
        assert_eq!(resolution_token(9999), "unspecified");
    }

    /// `M0` descends into `M1`, ... into `M{last}`: a chain of distinct types.
    fn chain(last: usize) -> DescriptorSet {
        let mut file = File::new("chain.proto", "d.v1");
        for i in 0..=last {
            let mut m = Msg::new(&format!("M{i}"));
            if i < last {
                m = m.field(
                    Fld::message("next", 1, &format!(".d.v1.M{}", i + 1)).conflict(ConflictUnit {
                        descend: true,
                        ..Default::default()
                    }),
                );
            }
            file = file.message(m);
        }
        Image::new().file(file).set()
    }

    #[test]
    fn descend_counts_depth_from_the_state_message() {
        let set = chain(2);
        let root = set.pool.get_message_by_name("d.v1.M0").unwrap();
        let reached: Vec<_> = descended(&root)
            .into_iter()
            .map(|(m, d)| (m.name().to_string(), d))
            .collect();
        assert_eq!(
            reached,
            vec![("M0".into(), 0), ("M1".into(), 1), ("M2".into(), 2)]
        );
    }

    #[test]
    fn a_message_at_the_depth_bound_is_reached_but_not_descended() {
        let set = chain(DESCEND_DEPTH + 1);
        let root = set.pool.get_message_by_name("d.v1.M0").unwrap();
        let reached = descended(&root);
        assert_eq!(reached.len(), DESCEND_DEPTH + 1);
        let (last, depth) = reached.last().unwrap();
        assert_eq!(last.name(), format!("M{DESCEND_DEPTH}"));
        assert_eq!(*depth, DESCEND_DEPTH);
    }

    #[test]
    fn the_model_serializes_with_kebab_tokens_and_no_nulls() {
        let model = build(&Image::new().file(counter()).set());
        let value = serde_json::to_value(&model).unwrap();
        let c = &value["components"][0];
        assert_eq!(c["kind"], "aggregate");
        assert_eq!(c["commands"][0]["type"], "c.v1.Increment");
        assert!(c.get("input_domain").is_none());
        assert!(c.get("state").is_none());
    }
}
