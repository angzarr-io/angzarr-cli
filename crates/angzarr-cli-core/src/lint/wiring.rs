//! The wiring rules: options are defined and decode, references resolve to
//! the right kind of component, each kind sets what it needs and nothing it
//! forbids, and the names a generator would emit do not collide.

use std::collections::{BTreeMap, BTreeSet};

use prost_reflect::MessageDescriptor;

use super::{Finding, Lint, Location};
use crate::DescriptorSet;
use crate::model::{
    Component, ComponentModel, Declared, Kind, base_name, declarations, field_marker, messages,
    output_domains,
};
use crate::options::{CONFLICT, MESSAGE_OPTIONS};

const MESSAGE_OPTIONS_TYPE: &str = "google.protobuf.MessageOptions";
const FIELD_OPTIONS_TYPE: &str = "google.protobuf.FieldOptions";

/// The method every projector's generated interface carries besides its
/// handlers.
pub const PROJECTOR_FINISH: &str = "Finish";

/// A message's generated type name: its name path within its package,
/// joined with `_` (`Outer_Inner`), as Go spells a nested type.
pub fn go_name(full_name: &str, package: &str) -> String {
    let local = if package.is_empty() {
        full_name
    } else {
        full_name
            .strip_prefix(package)
            .and_then(|s| s.strip_prefix('.'))
            .unwrap_or(full_name)
    };
    local.replace('.', "_")
}

/// `CamelCase` to `snake_case`: an upper-case letter starts a word after a
/// lower-case one, or ends an acronym when a lower-case letter follows it
/// (`HTTPGet` is `http_get`).
pub fn snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let after_lower = i > 0 && !chars[i - 1].is_uppercase();
            let ends_acronym = i > 0 && chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if after_lower || ends_acronym {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The first letter lower-cased (`camelCase` from `PascalCase`).
pub fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// How one target language spells a method name.
type Rendering = (&'static str, fn(&str) -> String);

/// The spellings of a method name per target language; two names collide
/// when any spelling coincides.
const RENDERINGS: [Rendering; 3] = [
    ("every-language", |n| n.to_string()),
    ("java-typescript", lower_first),
    ("python", snake),
];

pub(super) fn run(model: &ComponentModel, set: &DescriptorSet) -> Vec<Finding> {
    let declared = declarations(set);
    let mut out = Vec::new();
    options(set, &declared, &mut out);
    references(model, set, &declared, &mut out);
    kinds(&declared, &mut out);
    names(model, set, &declared, &mut out);
    out
}

fn message(set: &DescriptorSet, fq: &str) -> Option<MessageDescriptor> {
    set.pool.get_message_by_name(fq)
}

fn at(set: &DescriptorSet, fq: &str, file: &str) -> Location {
    message(set, fq).map_or_else(
        || Location {
            file: file.to_string(),
            message: fq.to_string(),
            field: None,
        },
        |m| Location::of_message(&m),
    )
}

/// Option bytes no file defines, and option bytes that do not decode.
fn options(set: &DescriptorSet, declared: &[Declared], out: &mut Vec<Finding>) {
    let defined: BTreeSet<(String, u32)> = set
        .pool
        .all_extensions()
        .map(|e| (e.containing_message().full_name().to_string(), e.number()))
        .collect();
    let is_defined = |on: &str, n: u32| defined.contains(&(on.to_string(), n));
    for d in declared {
        for &number in &d.opts.present {
            let lint = if !is_defined(MESSAGE_OPTIONS_TYPE, number) {
                Lint::OptionUndefined {
                    message: d.fq().to_string(),
                    number,
                }
            } else if d.opts.malformed.contains(&number) {
                Lint::MalformedOption {
                    message: d.fq().to_string(),
                    number,
                }
            } else {
                continue;
            };
            out.push(Finding {
                location: Location::of_message(&d.message),
                lint,
            });
        }
    }
    for m in messages(set) {
        for field in m.fields() {
            let marker = field_marker(&field);
            let lint = if !marker.present {
                continue;
            } else if !is_defined(FIELD_OPTIONS_TYPE, CONFLICT) {
                Lint::OptionUndefined {
                    message: m.full_name().to_string(),
                    number: CONFLICT,
                }
            } else if marker.malformed {
                Lint::MalformedOption {
                    message: m.full_name().to_string(),
                    number: CONFLICT,
                }
            } else {
                continue;
            };
            out.push(Finding {
                location: Location::of_field(&field),
                lint,
            });
        }
    }
    debug_assert!(MESSAGE_OPTIONS.iter().all(|n| *n != CONFLICT));
}

/// Commands and events point at a component of the right kind, emits name
/// messages in the set, triggers name a domain, and nothing is generated
/// apart from its anchor.
fn references(
    model: &ComponentModel,
    set: &DescriptorSet,
    declared: &[Declared],
    out: &mut Vec<Finding>,
) {
    let anchors: BTreeMap<&str, &Component> = model
        .components
        .iter()
        .map(|c| (c.anchor.as_str(), c))
        .collect();
    let registry: BTreeSet<String> = messages(set)
        .iter()
        .map(|m| m.full_name().to_string())
        .collect();
    for d in declared {
        let here = || Location::of_message(&d.message);
        let mut push = |lint| {
            out.push(Finding {
                location: here(),
                lint,
            })
        };
        let split = |owner: &Component| {
            (set.generated.contains(&d.file()) && !set.generated.contains(&owner.file)).then(|| {
                Lint::SplitRun {
                    message: d.fq().to_string(),
                    component: owner.anchor.clone(),
                    anchor_file: owner.file.clone(),
                }
            })
        };
        if let Some(cmd) = &d.opts.command {
            match anchors.get(cmd.component.as_str()) {
                None => push(Lint::UnknownComponent {
                    marker: "command",
                    message: d.fq().to_string(),
                    component: cmd.component.clone(),
                }),
                Some(owner) => {
                    if owner.kind != Kind::Aggregate {
                        push(Lint::CommandNotAggregate {
                            message: d.fq().to_string(),
                            component: owner.anchor.clone(),
                            kind: owner.kind.token(),
                        });
                    } else {
                        for emitted in cmd.emits.iter().filter(|e| !registry.contains(*e)) {
                            push(Lint::EmitsUnknown {
                                message: d.fq().to_string(),
                                emitted: emitted.clone(),
                            });
                        }
                    }
                    if let Some(lint) = split(owner) {
                        push(lint);
                    }
                }
            }
        }
        for ev in &d.opts.events {
            match anchors.get(ev.component.as_str()) {
                None => push(Lint::UnknownComponent {
                    marker: "event",
                    message: d.fq().to_string(),
                    component: ev.component.clone(),
                }),
                Some(owner) => {
                    if owner.kind == Kind::ProcessManager && !ev.applies && ev.domain.is_empty() {
                        push(Lint::TriggerWithoutDomain {
                            message: d.fq().to_string(),
                            component: owner.anchor.clone(),
                        });
                    }
                    if let Some(lint) = split(owner) {
                        push(lint);
                    }
                }
            }
        }
    }
}

/// Each kind's domain roles (options.proto, ComponentOptions):
///
/// ```text
/// kind             domain     input_domain  output_domain(s)
/// AGGREGATE        required   empty         empty
/// PROCESS_MANAGER  required   empty         targets
/// SAGA             forbidden  required      targets, at least one
/// PROJECTOR        forbidden  optional      empty
/// ```
fn kinds(declared: &[Declared], out: &mut Vec<Finding>) {
    for d in declared {
        let Some(c) = &d.opts.component else { continue };
        let kind = Kind::from_proto(c.kind);
        let component = d.fq().to_string();
        let outputs = output_domains(c).join(", ");
        let mut lints = Vec::new();
        let missing = |field| Lint::MissingField {
            component: component.clone(),
            kind: kind.token(),
            field,
        };
        let forbidden = |field, value: &str| {
            (!value.is_empty()).then(|| Lint::ForbiddenField {
                component: component.clone(),
                kind: kind.token(),
                field,
                value: value.to_string(),
            })
        };
        match kind {
            Kind::Aggregate => {
                lints.extend(c.domain.is_empty().then(|| missing("domain")));
                lints.extend(forbidden("input_domain", &c.input_domain));
                lints.extend(forbidden("output_domain", &outputs));
            }
            Kind::ProcessManager => {
                lints.extend(c.domain.is_empty().then(|| missing("domain")));
                lints.extend(forbidden("input_domain", &c.input_domain));
            }
            Kind::Saga => {
                lints.extend(forbidden("domain", &c.domain));
                lints.extend(c.input_domain.is_empty().then(|| missing("input_domain")));
                lints.extend(outputs.is_empty().then(|| missing("output_domain")));
            }
            Kind::Projector => {
                lints.extend(forbidden("domain", &c.domain));
                lints.extend(forbidden("output_domain", &outputs));
            }
            Kind::Unspecified => lints.push(Lint::UnspecifiedKind {
                component: component.clone(),
            }),
        }
        out.extend(lints.into_iter().map(|lint| Finding {
            location: Location::of_message(&d.message),
            lint,
        }));
    }
}

/// The methods a component's generated interface carries: one per command
/// and per reacted event (the message's name), one per applied event
/// (`Apply` and the name), and a projector's `Finish`.
fn methods(set: &DescriptorSet, c: &Component) -> Vec<String> {
    let name = |fq: &str| {
        let package = message(set, fq).map(|m| m.package_name().to_string());
        go_name(fq, package.as_deref().unwrap_or(""))
    };
    let mut out: Vec<String> = c.commands.iter().map(|cmd| name(&cmd.type_name)).collect();
    out.extend(c.events.iter().map(|e| {
        if e.applies {
            format!("Apply{}", name(&e.type_name))
        } else {
            name(&e.type_name)
        }
    }));
    if c.kind == Kind::Projector {
        out.push(PROJECTOR_FINISH.to_string());
    }
    out
}

/// The top-level names a generator emits for a component beside the proto
/// types: the scaffold stub, the handler interface, the wiring class, and
/// the dispatch constructor and registration function.
fn generated_types(stub: &str, base: &str) -> [String; 5] {
    [
        stub.to_string(),
        format!("{base}Handler"),
        format!("{base}Angzarr"),
        format!("New{base}Dispatch"),
        format!("Register{base}"),
    ]
}

fn names(
    model: &ComponentModel,
    set: &DescriptorSet,
    declared: &[Declared],
    out: &mut Vec<Finding>,
) {
    let mut first: BTreeMap<&str, &str> = BTreeMap::new();
    for c in &model.components {
        if let Some(other) = first.get(c.name.as_str()) {
            out.push(Finding {
                location: at(set, &c.anchor, &c.file),
                lint: Lint::NameCollision {
                    component: c.anchor.clone(),
                    name: c.name.clone(),
                    other: other.to_string(),
                },
            });
        } else {
            first.insert(&c.name, &c.anchor);
        }
    }

    for c in &model.components {
        let names = methods(set, c);
        for j in 1..names.len() {
            for i in 0..j {
                if let Some((language, rendered)) = RENDERINGS.iter().find_map(|(lang, render)| {
                    let r = render(&names[i]);
                    (r == render(&names[j])).then_some((*lang, r))
                }) {
                    out.push(Finding {
                        location: at(set, &c.anchor, &c.file),
                        lint: Lint::DuplicateMethod {
                            component: c.anchor.clone(),
                            method: rendered,
                            language,
                            first: names[i].clone(),
                            second: names[j].clone(),
                        },
                    });
                }
            }
        }
    }

    let mut types: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in set.pool.files() {
        let package = file.package_name().to_string();
        let names = types.entry(package.clone()).or_default();
        for e in file.enums() {
            names.insert(go_name(e.full_name(), &package));
        }
    }
    for m in messages(set) {
        let package = m.package_name().to_string();
        let names = types.entry(package.clone()).or_default();
        names.insert(go_name(m.full_name(), &package));
        for e in m.child_enums() {
            names.insert(go_name(e.full_name(), &package));
        }
    }
    for d in declared {
        let Some(opts) = &d.opts.component else {
            continue;
        };
        let base = base_name(&d.message, opts);
        let stub = if opts.name.is_empty() {
            format!("{}Impl", d.message.name())
        } else {
            opts.name.clone()
        };
        let package = d.message.package_name().to_string();
        let taken = types.get(&package);
        for generated in generated_types(&stub, &base) {
            if taken.is_some_and(|t| t.contains(&generated)) {
                out.push(Finding {
                    location: Location::of_message(&d.message),
                    lint: Lint::TypeCollision {
                        component: d.fq().to_string(),
                        generated,
                        package: package.clone(),
                    },
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::*;
    use crate::lint::tests::with_code;
    use crate::model::build;

    fn lint(image: Image) -> Vec<Finding> {
        let set = image.set();
        run(&build(&set), &set)
    }

    fn codes(findings: &[Finding]) -> Vec<&'static str> {
        use annals_diagnostic::Diagnostic;
        findings.iter().map(|f| f.lint.code()).collect()
    }

    #[test]
    fn a_well_formed_set_has_no_findings() {
        let projector = File::new("totals.proto", "t.v1")
            .dep("counter.proto")
            .message(Msg::new("Totals").component(component(
                ComponentKind::Projector,
                "",
                "counter",
                &[],
            )))
            .message(Msg::new("Seen").event(event("t.v1.Totals", "counter", false)));
        let findings = lint(Image::new().file(counter()).file(projector));
        assert!(findings.is_empty(), "{:?}", codes(&findings));
    }

    #[test]
    fn helpers_spell_names_as_the_generators_do() {
        assert_eq!(go_name("a.v1.Outer.Inner", "a.v1"), "Outer_Inner");
        assert_eq!(go_name("Top", ""), "Top");
        assert_eq!(snake("HTTPGet"), "http_get");
        assert_eq!(snake("HttpGet"), "http_get");
        assert_eq!(snake("Increment"), "increment");
        assert_eq!(lower_first("PlaceOrder"), "placeOrder");
    }

    #[test]
    fn option_bytes_no_file_defines_are_reported_once_per_option() {
        let findings = lint(Image::bare().file(counter()));
        let undefined = with_code(&findings, "angzarr-cli-core-option-undefined");
        assert_eq!(undefined.len(), 3, "{findings:?}");
        assert_eq!(
            undefined[0].lint,
            Lint::OptionUndefined {
                message: "c.v1.CounterState".into(),
                number: 50100
            }
        );
        assert_eq!(
            undefined[0].location.to_string(),
            "counter.proto#c.v1.CounterState"
        );

        let marked = File::new("m.proto", "m.v1").message(
            Msg::new("M").field(Fld::scalar("f", 1, ty::STRING).conflict(ConflictUnit::default())),
        );
        let findings = lint(Image::bare().file(marked));
        let undefined = with_code(&findings, "angzarr-cli-core-option-undefined");
        assert_eq!(undefined[0].location.to_string(), "m.proto#m.v1.M.f");
    }

    #[test]
    fn option_bytes_that_do_not_decode_are_malformed() {
        let bad = File::new("b.proto", "b.v1")
            .message(Msg::new("B").raw_options(&raw_option(50100, &[1, 0x80])));
        let findings = lint(Image::new().file(bad));
        assert_eq!(
            with_code(&findings, "angzarr-cli-core-malformed-option")[0].lint,
            Lint::MalformedOption {
                message: "b.v1.B".into(),
                number: 50100
            }
        );
    }

    #[test]
    fn a_reference_to_no_component_is_unknown() {
        let file = counter()
            .message(Msg::new("Lost").command(command("c.v1.Nowhere", &[])))
            .message(Msg::new("Stray").event(event("Counter", "", false)));
        let findings = lint(Image::new().file(file));
        let unknown = with_code(&findings, "angzarr-cli-core-unknown-component");
        assert_eq!(
            unknown.iter().map(|f| f.lint.clone()).collect::<Vec<_>>(),
            vec![
                Lint::UnknownComponent {
                    marker: "command",
                    message: "c.v1.Lost".into(),
                    component: "c.v1.Nowhere".into()
                },
                Lint::UnknownComponent {
                    marker: "event",
                    message: "c.v1.Stray".into(),
                    component: "Counter".into()
                },
            ]
        );
    }

    #[test]
    fn a_command_for_a_non_aggregate_is_reported() {
        let file = File::new("s.proto", "s.v1")
            .message(Msg::new("Flow").component(component(ComponentKind::Saga, "", "a", &["b"])))
            .message(Msg::new("Go").command(command("s.v1.Flow", &[])));
        let findings = lint(Image::new().file(file));
        assert_eq!(
            codes(&findings),
            vec!["angzarr-cli-core-command-not-aggregate"]
        );
        assert_eq!(
            findings[0].lint,
            Lint::CommandNotAggregate {
                message: "s.v1.Go".into(),
                component: "s.v1.Flow".into(),
                kind: "saga"
            }
        );
    }

    #[test]
    fn emits_must_be_fully_qualified_messages_in_the_set() {
        let file = counter().message(Msg::new("Double").command(command(
            "c.v1.CounterState",
            &["Incremented", "", "c.v1.Incremented"],
        )));
        let findings = lint(Image::new().file(file));
        let emits: Vec<_> = with_code(&findings, "angzarr-cli-core-emits-unknown")
            .into_iter()
            .map(|f| f.lint)
            .collect();
        assert_eq!(
            emits,
            vec![
                Lint::EmitsUnknown {
                    message: "c.v1.Double".into(),
                    emitted: "Incremented".into()
                },
                Lint::EmitsUnknown {
                    message: "c.v1.Double".into(),
                    emitted: String::new()
                },
            ]
        );
    }

    #[test]
    fn a_process_manager_trigger_needs_a_domain() {
        let file = File::new("pm.proto", "p.v1")
            .message(Msg::new("Flow").component(component(
                ComponentKind::ProcessManager,
                "flow",
                "",
                &[],
            )))
            .message(Msg::new("Folded").event(event("p.v1.Flow", "", true)))
            .message(Msg::new("Trigger").event(event("p.v1.Flow", "", false)))
            .message(Msg::new("Sourced").event(event("p.v1.Flow", "billing", false)));
        let findings = lint(Image::new().file(file));
        assert_eq!(
            codes(&findings),
            vec!["angzarr-cli-core-trigger-without-domain"]
        );
        assert_eq!(findings[0].location.message, "p.v1.Trigger");
    }

    #[test]
    fn each_kind_requires_and_forbids_its_domain_roles() {
        let agg = component(ComponentKind::Aggregate, "", "x", &["y"]);
        let pm = component(ComponentKind::ProcessManager, "", "x", &["y"]);
        let saga = component(ComponentKind::Saga, "own", "", &[]);
        let proj = component(ComponentKind::Projector, "own", "", &["y"]);
        let none = component(ComponentKind::Unspecified, "", "", &[]);
        let file = File::new("k.proto", "k.v1")
            .message(Msg::new("A").component(agg))
            .message(Msg::new("P").component(pm))
            .message(Msg::new("S").component(saga))
            .message(Msg::new("J").component(proj))
            .message(Msg::new("U").component(none));
        let findings = lint(Image::new().file(file));
        let got: Vec<_> = findings
            .iter()
            .filter_map(|f| match &f.lint {
                Lint::MissingField {
                    component, field, ..
                } => Some(format!("{component} missing {field}")),
                Lint::ForbiddenField {
                    component,
                    field,
                    value,
                    ..
                } => Some(format!("{component} forbids {field}={value}")),
                Lint::UnspecifiedKind { component } => Some(format!("{component} unspecified")),
                _ => None,
            })
            .collect();
        assert_eq!(
            got,
            vec![
                "k.v1.A missing domain",
                "k.v1.A forbids input_domain=x",
                "k.v1.A forbids output_domain=y",
                "k.v1.P missing domain",
                "k.v1.P forbids input_domain=x",
                "k.v1.S forbids domain=own",
                "k.v1.S missing input_domain",
                "k.v1.S missing output_domain",
                "k.v1.J forbids domain=own",
                "k.v1.J forbids output_domain=y",
                "k.v1.U unspecified",
            ]
        );
    }

    #[test]
    fn two_components_may_not_share_a_generated_name() {
        let mut named = component(ComponentKind::Aggregate, "other", "", &[]);
        named.name = "CounterState".into();
        let file = File::new("o.proto", "o.v1").message(Msg::new("Other").component(named));
        let findings = lint(Image::new().file(counter()).file(file));
        let collisions = with_code(&findings, "angzarr-cli-core-name-collision");
        assert_eq!(
            collisions[0].lint,
            Lint::NameCollision {
                component: "o.v1.Other".into(),
                name: "CounterState".into(),
                other: "c.v1.CounterState".into()
            }
        );
    }

    #[test]
    fn one_component_may_not_generate_a_method_twice_in_any_language() {
        let file = counter()
            .message(Msg::new("HTTPGet").command(command("c.v1.CounterState", &[])))
            .message(Msg::new("HttpGet").command(command("c.v1.CounterState", &[])));
        let findings = lint(Image::new().file(file));
        let dups = with_code(&findings, "angzarr-cli-core-duplicate-method");
        assert_eq!(dups.len(), 1, "{findings:?}");
        assert_eq!(
            dups[0].lint,
            Lint::DuplicateMethod {
                component: "c.v1.CounterState".into(),
                method: "http_get".into(),
                language: "python",
                first: "HTTPGet".into(),
                second: "HttpGet".into()
            }
        );

        // A projector's handler named Finish meets the fixed Finish.
        let proj = File::new("p.proto", "p.v1")
            .message(Msg::new("View").component(component(ComponentKind::Projector, "", "a", &[])))
            .message(Msg::new("Finish").event(event("p.v1.View", "a", false)));
        let findings = lint(Image::new().file(proj));
        assert_eq!(
            with_code(&findings, "angzarr-cli-core-duplicate-method")[0].lint,
            Lint::DuplicateMethod {
                component: "p.v1.View".into(),
                method: "Finish".into(),
                language: "every-language",
                first: "Finish".into(),
                second: "Finish".into()
            }
        );
    }

    #[test]
    fn a_generated_type_may_not_collide_with_a_proto_type() {
        let file = counter()
            .message(Msg::new("CounterStateHandler"))
            .enum_type("RegisterCounterState");
        let findings = lint(Image::new().file(file));
        let generated: Vec<_> = with_code(&findings, "angzarr-cli-core-type-collision")
            .into_iter()
            .map(|f| match f.lint {
                Lint::TypeCollision {
                    generated, package, ..
                } => format!("{package}.{generated}"),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            generated,
            vec!["c.v1.CounterStateHandler", "c.v1.RegisterCounterState"]
        );

        let nested = File::new("n.proto", "n.v1").message(
            Msg::new("Box")
                .component(component(ComponentKind::Aggregate, "box", "", &[]))
                .nested_enum("Impl"),
        );
        let findings = lint(
            Image::new()
                .file(nested)
                .file(File::new("n2.proto", "n.v1").message(Msg::new("BoxImpl"))),
        );
        assert_eq!(
            with_code(&findings, "angzarr-cli-core-type-collision").len(),
            1,
            "{findings:?}"
        );
    }

    #[test]
    fn a_command_generated_without_its_anchor_is_a_split_run() {
        let commands = File::new("cmds.proto", "c.v1")
            .dep("counter.proto")
            .message(Msg::new("Decrement").command(command("c.v1.CounterState", &[])));
        let findings = lint(Image::new().import(counter()).file(commands));
        assert_eq!(codes(&findings), vec!["angzarr-cli-core-split-run"]);
        assert_eq!(
            findings[0].lint,
            Lint::SplitRun {
                message: "c.v1.Decrement".into(),
                component: "c.v1.CounterState".into(),
                anchor_file: "counter.proto".into()
            }
        );

        // Both generated, or the command only imported: no split.
        let both = File::new("cmds.proto", "c.v1")
            .dep("counter.proto")
            .message(Msg::new("Decrement").command(command("c.v1.CounterState", &[])));
        assert!(lint(Image::new().file(counter()).file(both.clone())).is_empty());
        assert!(lint(Image::new().import(counter()).import(both)).is_empty());
    }
}
