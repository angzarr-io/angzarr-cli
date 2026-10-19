//! The conflict rules: where `(state)` may go, what it may list, and the
//! `(conflict)` markers of a COMMUTATIVE state, each as options.proto's
//! conflict_units region states it ("Generation diagnostics").

use std::collections::{BTreeMap, BTreeSet};

use angzarr_proto::v1::{ConflictResolution, ConflictUnit};
use prost_reflect::{FieldDescriptor, Kind as FieldKind, MessageDescriptor, OneofDescriptor};

use super::{Finding, Lint, Location};
use crate::DescriptorSet;
use crate::model::{
    ComponentModel, declarations, descend_valid, descended, field_marker, messages,
};

pub(super) fn run(model: &ComponentModel, set: &DescriptorSet) -> Vec<Finding> {
    let kinds: BTreeMap<&str, _> = model
        .components
        .iter()
        .map(|c| (c.anchor.as_str(), c.kind))
        .collect();
    let mut out = Vec::new();
    let mut commutative = Vec::new();
    for d in declarations(set) {
        let Some(state) = &d.opts.state else { continue };
        let here = Location::of_message(&d.message);
        let kind = kinds.get(d.fq()).copied();
        if !kind.is_some_and(|k| k.is_stateful()) {
            out.push(Finding {
                location: here.clone(),
                lint: Lint::StateNotOnState {
                    message: d.fq().to_string(),
                    kind: kind.map_or("none", |k| k.token()),
                },
            });
        }
        let listed = &state.conflict_resolutions;
        if listed.contains(&(ConflictResolution::Unspecified as i32)) {
            out.push(Finding {
                location: here,
                lint: Lint::UnspecifiedResolution {
                    message: d.fq().to_string(),
                },
            });
        }
        if listed.contains(&(ConflictResolution::Commutative as i32)) {
            commutative.push(d.message.clone());
        }
    }

    let mut reached = Vec::new();
    let mut seen = BTreeSet::new();
    for state in &commutative {
        if !state.fields().any(|f| field_marker(&f).present) {
            out.push(Finding {
                location: Location::of_message(state),
                lint: Lint::NoMarkedFields {
                    message: state.full_name().to_string(),
                },
            });
        }
        for (m, _) in descended(state) {
            if seen.insert(m.full_name().to_string()) {
                reached.push(m);
            }
        }
    }
    for m in &reached {
        units(m, &mut out);
    }
    for m in messages(set) {
        if seen.contains(m.full_name()) {
            continue;
        }
        for field in m.fields().filter(|f| field_marker(f).present) {
            out.push(Finding {
                location: Location::of_field(&field),
                lint: Lint::MarkerOutsideState {
                    message: m.full_name().to_string(),
                    field: field.name().to_string(),
                },
            });
        }
    }
    out
}

/// A real oneof: not the synthetic one of a proto3 `optional` field.
fn real_oneof(field: &FieldDescriptor) -> Option<OneofDescriptor> {
    field.containing_oneof().filter(|o| !o.is_synthetic())
}

fn oneof_location(m: &MessageDescriptor, oneof: &OneofDescriptor) -> Location {
    Location {
        field: Some(oneof.name().to_string()),
        ..Location::of_message(m)
    }
}

/// The markers of one message a COMMUTATIVE state reaches.
fn units(m: &MessageDescriptor, out: &mut Vec<Finding>) {
    let message = m.full_name().to_string();
    for oneof in m.oneofs().filter(|o| !o.is_synthetic()) {
        let markers: Vec<_> = oneof.fields().map(|f| field_marker(&f)).collect();
        let marked = markers.iter().filter(|o| o.present).count();
        let lint = if marked == 0 {
            Lint::UnmarkedField {
                message: message.clone(),
                field: oneof.name().to_string(),
            }
        } else if marked < markers.len() {
            Lint::OneofPartiallyMarked {
                message: message.clone(),
                oneof: oneof.name().to_string(),
            }
        } else {
            let units: Vec<ConflictUnit> = markers
                .into_iter()
                .map(|o| o.conflict.unwrap_or_default())
                .collect();
            let refined = units
                .iter()
                .any(|u| u.descend || !u.element_key.is_empty() || u.ordered);
            let whole = units.iter().filter(|u| u.whole_state).count();
            if !refined && (whole == 0 || whole == units.len()) {
                continue;
            }
            Lint::OneofMemberRefined {
                message: message.clone(),
                oneof: oneof.name().to_string(),
            }
        };
        out.push(Finding {
            location: oneof_location(m, &oneof),
            lint,
        });
    }
    for field in m.fields().filter(|f| real_oneof(f).is_none()) {
        let marker = field_marker(&field);
        let here = || Location::of_field(&field);
        let name = field.name().to_string();
        if !marker.present {
            out.push(Finding {
                location: here(),
                lint: Lint::UnmarkedField {
                    message: message.clone(),
                    field: name,
                },
            });
            continue;
        }
        let Some(unit) = marker.conflict else {
            continue;
        };
        let mut lints = Vec::new();
        if unit.whole_state && (unit.descend || !unit.element_key.is_empty() || unit.ordered) {
            lints.push(Lint::WholeStateCombined {
                message: message.clone(),
                field: name.clone(),
            });
        }
        if unit.ordered && unit.element_key.is_empty() {
            lints.push(Lint::OrderedWithoutKey {
                message: message.clone(),
                field: name.clone(),
            });
        }
        if !unit.element_key.is_empty() && !element_key_valid(&field, &unit.element_key) {
            lints.push(Lint::BadElementKey {
                message: message.clone(),
                field: name.clone(),
                key: unit.element_key.clone(),
            });
        }
        if unit.descend && !descend_valid(&field, &unit) {
            lints.push(Lint::DescendNonMessage {
                message: message.clone(),
                field: name.clone(),
            });
        }
        out.extend(lints.into_iter().map(|lint| Finding {
            location: here(),
            lint,
        }));
    }
}

/// `element_key` belongs on a repeated message field (not a map) and names
/// a singular scalar, string, bytes or enum field of the element.
fn element_key_valid(field: &FieldDescriptor, key: &str) -> bool {
    if !field.is_list() {
        return false;
    }
    let FieldKind::Message(element) = field.kind() else {
        return false;
    };
    element
        .get_field_by_name(key)
        .is_some_and(|k| !k.is_list() && !k.is_map() && !matches!(k.kind(), FieldKind::Message(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::*;
    use crate::model::build;

    fn lint(file: File) -> Vec<Finding> {
        let set = Image::new().file(file).set();
        run(&build(&set), &set)
    }

    fn agg() -> ComponentOptions {
        component(ComponentKind::Aggregate, "list", "", &[])
    }

    fn commutative() -> StateOptions {
        state(&[ConflictResolution::Commutative])
    }

    fn mark() -> ConflictUnit {
        ConflictUnit::default()
    }

    fn unit(descend: bool, key: &str, ordered: bool, whole: bool) -> ConflictUnit {
        ConflictUnit {
            descend,
            element_key: key.into(),
            ordered,
            whole_state: whole,
        }
    }

    /// Each finding as `code location`.
    fn found(findings: &[Finding]) -> Vec<String> {
        use annals_diagnostic::Diagnostic;
        findings
            .iter()
            .map(|f| {
                format!(
                    "{} {}",
                    f.lint.code().trim_start_matches("angzarr-cli-core-"),
                    f.location
                )
            })
            .collect()
    }

    #[test]
    fn a_fully_marked_commutative_state_is_clean() {
        let file = File::new("l.proto", "l.v1")
            .message(
                Msg::new("ListState")
                    .component(agg())
                    .state(commutative())
                    .nested(Msg::map_entry("ItemsEntry", ty::STRING, Some(".l.v1.Item")))
                    .field(Fld::scalar("title", 1, ty::STRING).conflict(mark()))
                    .field(
                        Fld::message("items", 2, ".l.v1.ListState.ItemsEntry")
                            .repeated()
                            .conflict(unit(true, "", false, false)),
                    )
                    .field(
                        Fld::message("columns", 3, ".l.v1.Column")
                            .repeated()
                            .conflict(unit(false, "id", true, false)),
                    )
                    .field(
                        Fld::scalar("notes", 4, ty::STRING).conflict(unit(false, "", false, true)),
                    )
                    .oneof("payment")
                    .field(
                        Fld::scalar("card", 5, ty::STRING)
                            .in_oneof(0)
                            .conflict(mark()),
                    )
                    .field(
                        Fld::scalar("cash", 6, ty::BOOL)
                            .in_oneof(0)
                            .conflict(mark()),
                    ),
            )
            .message(Msg::new("Item").field(Fld::scalar("weight", 1, ty::INT64).conflict(mark())))
            .message(Msg::new("Column").field(Fld::scalar("id", 1, ty::STRING)));
        assert_eq!(found(&lint(file)), Vec::<String>::new());
    }

    #[test]
    fn state_belongs_on_an_aggregate_or_process_manager_state() {
        let file = File::new("s.proto", "s.v1")
            .message(
                Msg::new("Flow")
                    .component(component(ComponentKind::Saga, "", "a", &["b"]))
                    .state(state(&[])),
            )
            .message(Msg::new("Loose").state(state(&[])))
            .message(
                Msg::new("PmState")
                    .component(component(ComponentKind::ProcessManager, "pm", "", &[]))
                    .state(state(&[ConflictResolution::Strict])),
            );
        let findings = lint(file);
        assert_eq!(
            findings.iter().map(|f| f.lint.clone()).collect::<Vec<_>>(),
            vec![
                Lint::StateNotOnState {
                    message: "s.v1.Flow".into(),
                    kind: "saga"
                },
                Lint::StateNotOnState {
                    message: "s.v1.Loose".into(),
                    kind: "none"
                },
            ]
        );
    }

    #[test]
    fn unspecified_may_not_be_listed() {
        let file = File::new("s.proto", "s.v1").message(
            Msg::new("S")
                .component(agg())
                .state(state(&[ConflictResolution::Unspecified])),
        );
        assert_eq!(
            found(&lint(file)),
            vec!["unspecified-resolution s.proto#s.v1.S"]
        );
    }

    #[test]
    fn a_commutative_state_marks_at_least_one_field_and_warns_on_the_rest() {
        let file = File::new("s.proto", "s.v1").message(
            Msg::new("S")
                .component(agg())
                .state(commutative())
                .field(Fld::scalar("a", 1, ty::STRING)),
        );
        assert_eq!(
            found(&lint(file)),
            vec![
                "no-marked-fields s.proto#s.v1.S",
                "unmarked-field s.proto#s.v1.S.a"
            ]
        );

        let file = File::new("s.proto", "s.v1").message(
            Msg::new("S")
                .component(agg())
                .state(commutative())
                .field(Fld::scalar("a", 1, ty::STRING).conflict(mark()))
                .field(Fld::scalar("b", 2, ty::STRING)),
        );
        let findings = lint(file);
        assert_eq!(found(&findings), vec!["unmarked-field s.proto#s.v1.S.b"]);
        assert_eq!(findings[0].severity(), super::super::Severity::Warning);
    }

    #[test]
    fn a_descended_message_is_checked_like_the_state() {
        let file = File::new("s.proto", "s.v1")
            .message(Msg::new("S").component(agg()).state(commutative()).field(
                Fld::message("inner", 1, ".s.v1.Inner").conflict(unit(true, "", false, false)),
            ))
            .message(Msg::new("Inner").field(Fld::scalar("x", 1, ty::STRING)));
        assert_eq!(
            found(&lint(file)),
            vec!["unmarked-field s.proto#s.v1.Inner.x"]
        );
    }

    #[test]
    fn a_marker_outside_a_commutative_state_is_an_error() {
        // On a state that does not list COMMUTATIVE.
        let file = File::new("s.proto", "s.v1").message(
            Msg::new("S")
                .component(agg())
                .field(Fld::scalar("a", 1, ty::STRING).conflict(mark())),
        );
        assert_eq!(
            found(&lint(file)),
            vec!["marker-outside-state s.proto#s.v1.S.a"]
        );

        // On a message the state holds but does not descend into.
        let file = File::new("s.proto", "s.v1")
            .message(
                Msg::new("S")
                    .component(agg())
                    .state(commutative())
                    .field(Fld::message("inner", 1, ".s.v1.Inner").conflict(mark())),
            )
            .message(Msg::new("Inner").field(Fld::scalar("x", 1, ty::STRING).conflict(mark())));
        assert_eq!(
            found(&lint(file)),
            vec!["marker-outside-state s.proto#s.v1.Inner.x"]
        );
    }

    #[test]
    fn descend_needs_a_message_a_map_of_messages_or_a_keyed_list() {
        let file = File::new("s.proto", "s.v1").message(
            Msg::new("S")
                .component(agg())
                .state(commutative())
                .nested(Msg::map_entry("TagsEntry", ty::STRING, None))
                .nested(Msg::new("E").field(Fld::scalar("id", 1, ty::STRING).conflict(mark())))
                .field(Fld::scalar("a", 1, ty::STRING).conflict(unit(true, "", false, false)))
                .field(
                    Fld::message("tags", 2, ".s.v1.S.TagsEntry")
                        .repeated()
                        .conflict(unit(true, "", false, false)),
                )
                .field(
                    Fld::message("list", 3, ".s.v1.S.E")
                        .repeated()
                        .conflict(unit(true, "", false, false)),
                ),
        );
        assert_eq!(
            found(&lint(file)),
            vec![
                "descend-non-message s.proto#s.v1.S.a",
                "descend-non-message s.proto#s.v1.S.tags",
                "descend-non-message s.proto#s.v1.S.list",
                "marker-outside-state s.proto#s.v1.S.E.id",
            ]
        );
    }

    #[test]
    fn element_key_names_a_singular_scalar_of_a_repeated_message_element() {
        let file = File::new("s.proto", "s.v1")
            .message(
                Msg::new("S")
                    .component(agg())
                    .state(commutative())
                    .field(
                        Fld::scalar("a", 1, ty::STRING).conflict(unit(false, "id", false, false)),
                    )
                    .field(
                        Fld::message("missing", 2, ".s.v1.E")
                            .repeated()
                            .conflict(unit(false, "nope", false, false)),
                    )
                    .field(
                        Fld::message("listy", 3, ".s.v1.E")
                            .repeated()
                            .conflict(unit(false, "tags", false, false)),
                    )
                    .field(
                        Fld::message("good", 4, ".s.v1.E")
                            .repeated()
                            .conflict(unit(false, "id", true, false)),
                    ),
            )
            .message(
                Msg::new("E")
                    .field(Fld::scalar("id", 1, ty::STRING))
                    .field(Fld::scalar("tags", 2, ty::STRING).repeated()),
            );
        assert_eq!(
            found(&lint(file)),
            vec![
                "bad-element-key s.proto#s.v1.S.a",
                "bad-element-key s.proto#s.v1.S.missing",
                "bad-element-key s.proto#s.v1.S.listy",
            ]
        );
    }

    #[test]
    fn ordered_needs_a_key_and_whole_state_stands_alone() {
        let file = File::new("s.proto", "s.v1").message(
            Msg::new("S")
                .component(agg())
                .state(commutative())
                .field(Fld::scalar("a", 1, ty::STRING).conflict(unit(false, "", true, false)))
                .field(Fld::scalar("b", 2, ty::STRING).conflict(unit(false, "", true, true)))
                .field(Fld::message("c", 3, ".s.v1.S").conflict(unit(true, "", false, true))),
        );
        assert_eq!(
            found(&lint(file)),
            vec![
                "ordered-without-key s.proto#s.v1.S.a",
                "whole-state-combined s.proto#s.v1.S.b",
                "ordered-without-key s.proto#s.v1.S.b",
                "whole-state-combined s.proto#s.v1.S.c",
            ]
        );
    }

    #[test]
    fn a_oneof_is_marked_whole_with_plain_or_all_whole_state_members() {
        let oneof = |a: Option<ConflictUnit>, b: Option<ConflictUnit>| {
            let fa = Fld::scalar("a", 1, ty::STRING).in_oneof(0);
            let fb = Fld::scalar("b", 2, ty::STRING).in_oneof(0);
            let fa = match a {
                Some(u) => fa.conflict(u),
                None => fa,
            };
            let fb = match b {
                Some(u) => fb.conflict(u),
                None => fb,
            };
            File::new("s.proto", "s.v1").message(
                Msg::new("S")
                    .component(agg())
                    .state(commutative())
                    .field(Fld::scalar("x", 3, ty::STRING).conflict(mark()))
                    .oneof("pay")
                    .field(fa)
                    .field(fb),
            )
        };
        assert_eq!(
            found(&lint(oneof(None, None))),
            vec!["unmarked-field s.proto#s.v1.S.pay"]
        );
        assert_eq!(
            found(&lint(oneof(Some(mark()), None))),
            vec!["oneof-partially-marked s.proto#s.v1.S.pay"]
        );
        let whole = || unit(false, "", false, true);
        assert!(lint(oneof(Some(whole()), Some(whole()))).is_empty());
        assert_eq!(
            found(&lint(oneof(Some(whole()), Some(mark())))),
            vec!["oneof-member-refined s.proto#s.v1.S.pay"]
        );
        assert_eq!(
            found(&lint(oneof(
                Some(unit(true, "", false, false)),
                Some(mark())
            ))),
            vec!["oneof-member-refined s.proto#s.v1.S.pay"]
        );
        assert_eq!(
            found(&lint(oneof(
                Some(unit(false, "", true, false)),
                Some(mark())
            ))),
            vec!["oneof-member-refined s.proto#s.v1.S.pay"]
        );
    }
}
