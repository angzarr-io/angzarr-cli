//! Descriptor-set fixtures, built in code: files, messages and fields with
//! the angzarr options encoded as extension bytes, written as a buf-style
//! image (each file may be marked an import) or loaded straight into a
//! [`DescriptorSet`].
//!
//! The descriptor messages here mirror the subset of
//! `google/protobuf/descriptor.proto` the fixtures need, with each options
//! message carried as its raw bytes: the wire form is the same, and the
//! bytes can hold extensions.

use std::collections::BTreeSet;

use prost::Message;
use prost::encoding::{WireType, encode_key, encode_varint};
use prost_reflect::DescriptorPool;

use crate::DescriptorSet;
use crate::options::{COMMAND, COMPONENT, CONFLICT, EVENT, STATE};

pub use angzarr_proto::v1::{
    CommandOptions, ComponentKind, ComponentOptions, ConflictResolution, ConflictUnit,
    EventConsumer, StateOptions,
};

/// Field types, as `FieldDescriptorProto.Type` numbers.
pub mod ty {
    pub const BOOL: i32 = 8;
    pub const STRING: i32 = 9;
    pub const MESSAGE: i32 = 11;
    pub const BYTES: i32 = 12;
    pub const INT64: i32 = 3;
    pub const ENUM: i32 = 14;
}

const LABEL_OPTIONAL: i32 = 1;
const LABEL_REPEATED: i32 = 3;

/// One option as raw bytes: `number`, length-delimited, holding `value`.
pub fn option<M: Message>(number: u32, value: &M) -> Vec<u8> {
    raw_option(number, &value.encode_to_vec())
}

/// One length-delimited option holding exactly `bytes`.
pub fn raw_option(number: u32, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    encode_key(number, WireType::LengthDelimited, &mut out);
    encode_varint(bytes.len() as u64, &mut out);
    out.extend_from_slice(bytes);
    out
}

/// One varint option.
pub fn varint_option(number: u32, value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    encode_key(number, WireType::Varint, &mut out);
    encode_varint(value, &mut out);
    out
}

#[derive(Clone, PartialEq, Message)]
struct FileSetProto {
    #[prost(bytes = "vec", repeated, tag = "1")]
    file: Vec<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
struct BufExt {
    #[prost(bool, optional, tag = "1")]
    is_import: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
struct FileProto {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
    #[prost(string, optional, tag = "2")]
    package: Option<String>,
    #[prost(string, repeated, tag = "3")]
    dependency: Vec<String>,
    #[prost(message, repeated, tag = "4")]
    message_type: Vec<MsgProto>,
    #[prost(message, repeated, tag = "5")]
    enum_type: Vec<EnumProto>,
    #[prost(message, repeated, tag = "7")]
    extension: Vec<FieldProto>,
    #[prost(string, optional, tag = "12")]
    syntax: Option<String>,
    #[prost(message, optional, tag = "8042")]
    buf: Option<BufExt>,
}

#[derive(Clone, PartialEq, Message)]
struct MsgProto {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
    #[prost(message, repeated, tag = "2")]
    field: Vec<FieldProto>,
    #[prost(message, repeated, tag = "3")]
    nested_type: Vec<MsgProto>,
    #[prost(message, repeated, tag = "4")]
    enum_type: Vec<EnumProto>,
    #[prost(bytes = "vec", optional, tag = "7")]
    options: Option<Vec<u8>>,
    #[prost(message, repeated, tag = "8")]
    oneof_decl: Vec<OneofProto>,
}

#[derive(Clone, PartialEq, Message)]
struct FieldProto {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
    #[prost(string, optional, tag = "2")]
    extendee: Option<String>,
    #[prost(int32, optional, tag = "3")]
    number: Option<i32>,
    #[prost(int32, optional, tag = "4")]
    label: Option<i32>,
    #[prost(int32, optional, tag = "5")]
    r#type: Option<i32>,
    #[prost(string, optional, tag = "6")]
    type_name: Option<String>,
    #[prost(bytes = "vec", optional, tag = "8")]
    options: Option<Vec<u8>>,
    #[prost(int32, optional, tag = "9")]
    oneof_index: Option<i32>,
    #[prost(bool, optional, tag = "17")]
    proto3_optional: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
struct OneofProto {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
struct EnumProto {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
    #[prost(message, repeated, tag = "2")]
    value: Vec<EnumValueProto>,
}

#[derive(Clone, PartialEq, Message)]
struct EnumValueProto {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
    #[prost(int32, optional, tag = "2")]
    number: Option<i32>,
}

fn enum_proto(name: &str) -> EnumProto {
    EnumProto {
        name: Some(name.into()),
        value: vec![EnumValueProto {
            name: Some(format!("{}_UNSPECIFIED", name.to_uppercase())),
            number: Some(0),
        }],
    }
}

/// A field of a fixture message.
#[derive(Clone, Debug)]
pub struct Fld {
    name: String,
    number: i32,
    label: i32,
    ty: i32,
    type_name: Option<String>,
    options: Vec<u8>,
    oneof: Option<i32>,
}

impl Fld {
    /// A singular scalar field of type `ty` (see [`ty`]).
    pub fn scalar(name: &str, number: i32, ty: i32) -> Fld {
        Fld {
            name: name.into(),
            number,
            label: LABEL_OPTIONAL,
            ty,
            type_name: None,
            options: Vec::new(),
            oneof: None,
        }
    }

    /// A singular message field of type `type_name` (fully qualified, with
    /// its leading dot).
    pub fn message(name: &str, number: i32, type_name: &str) -> Fld {
        Fld {
            type_name: Some(type_name.into()),
            ..Fld::scalar(name, number, ty::MESSAGE)
        }
    }

    /// The field, repeated.
    pub fn repeated(mut self) -> Fld {
        self.label = LABEL_REPEATED;
        self
    }

    /// The field, a member of the message's `index`th oneof.
    pub fn in_oneof(mut self, index: i32) -> Fld {
        self.oneof = Some(index);
        self
    }

    /// The field, marked `(conflict) = unit`.
    pub fn conflict(mut self, unit: ConflictUnit) -> Fld {
        self.options.extend(option(CONFLICT, &unit));
        self
    }

    /// The field, with raw option bytes appended.
    pub fn raw_options(mut self, bytes: &[u8]) -> Fld {
        self.options.extend_from_slice(bytes);
        self
    }

    fn proto(&self) -> FieldProto {
        FieldProto {
            name: Some(self.name.clone()),
            extendee: None,
            number: Some(self.number),
            label: Some(self.label),
            r#type: Some(self.ty),
            type_name: self.type_name.clone(),
            options: (!self.options.is_empty()).then(|| self.options.clone()),
            oneof_index: self.oneof,
            proto3_optional: None,
        }
    }
}

/// A fixture message.
#[derive(Clone, Debug)]
pub struct Msg {
    name: String,
    options: Vec<u8>,
    fields: Vec<Fld>,
    nested: Vec<Msg>,
    enums: Vec<String>,
    oneofs: Vec<String>,
}

impl Msg {
    pub fn new(name: &str) -> Msg {
        Msg {
            name: name.into(),
            options: Vec::new(),
            fields: Vec::new(),
            nested: Vec::new(),
            enums: Vec::new(),
            oneofs: Vec::new(),
        }
    }

    /// A map entry message `name` (`FooEntry`) with a string key and a value
    /// of type `ty`, or of message type `type_name` when given.
    pub fn map_entry(name: &str, ty: i32, type_name: Option<&str>) -> Msg {
        let value = match type_name {
            Some(t) => Fld::message("value", 2, t),
            None => Fld::scalar("value", 2, ty),
        };
        Msg::new(name)
            .raw_options(&varint_option(7, 1))
            .field(Fld::scalar("key", 1, ty::STRING))
            .field(value)
    }

    /// `(component) = c`.
    pub fn component(self, c: ComponentOptions) -> Msg {
        self.raw_options(&option(COMPONENT, &c))
    }

    /// `(command) = c`.
    pub fn command(self, c: CommandOptions) -> Msg {
        self.raw_options(&option(COMMAND, &c))
    }

    /// One more `(event)` entry.
    pub fn event(self, e: EventConsumer) -> Msg {
        self.raw_options(&option(EVENT, &e))
    }

    /// `(state) = s`.
    pub fn state(self, s: StateOptions) -> Msg {
        self.raw_options(&option(STATE, &s))
    }

    /// Raw option bytes appended.
    pub fn raw_options(mut self, bytes: &[u8]) -> Msg {
        self.options.extend_from_slice(bytes);
        self
    }

    pub fn field(mut self, f: Fld) -> Msg {
        self.fields.push(f);
        self
    }

    pub fn nested(mut self, m: Msg) -> Msg {
        self.nested.push(m);
        self
    }

    /// A nested enum with one value.
    pub fn nested_enum(mut self, name: &str) -> Msg {
        self.enums.push(name.into());
        self
    }

    /// A oneof; members name it by index with [`Fld::in_oneof`].
    pub fn oneof(mut self, name: &str) -> Msg {
        self.oneofs.push(name.into());
        self
    }

    fn proto(&self) -> MsgProto {
        MsgProto {
            name: Some(self.name.clone()),
            field: self.fields.iter().map(Fld::proto).collect(),
            nested_type: self.nested.iter().map(Msg::proto).collect(),
            enum_type: self.enums.iter().map(|e| enum_proto(e)).collect(),
            options: (!self.options.is_empty()).then(|| self.options.clone()),
            oneof_decl: self
                .oneofs
                .iter()
                .map(|o| OneofProto {
                    name: Some(o.clone()),
                })
                .collect(),
        }
    }
}

/// A fixture proto file.
#[derive(Clone, Debug)]
pub struct File {
    name: String,
    package: String,
    deps: Vec<String>,
    messages: Vec<Msg>,
    enums: Vec<String>,
}

impl File {
    pub fn new(name: &str, package: &str) -> File {
        File {
            name: name.into(),
            package: package.into(),
            deps: Vec::new(),
            messages: Vec::new(),
            enums: Vec::new(),
        }
    }

    pub fn message(mut self, m: Msg) -> File {
        self.messages.push(m);
        self
    }

    /// A top-level enum with one value.
    pub fn enum_type(mut self, name: &str) -> File {
        self.enums.push(name.into());
        self
    }

    /// An import of another fixture file.
    pub fn dep(mut self, name: &str) -> File {
        self.deps.push(name.into());
        self
    }

    fn proto(&self, is_import: bool) -> FileProto {
        FileProto {
            name: Some(self.name.clone()),
            package: Some(self.package.clone()),
            dependency: self.deps.clone(),
            message_type: self.messages.iter().map(Msg::proto).collect(),
            enum_type: self.enums.iter().map(|e| enum_proto(e)).collect(),
            extension: Vec::new(),
            syntax: Some("proto3".into()),
            buf: is_import.then_some(BufExt {
                is_import: Some(true),
            }),
        }
    }
}

/// The import path of the fixture's `options.proto`.
pub const OPTIONS_PROTO: &str = "io/angzarr/v1/options.proto";
const DESCRIPTOR_PROTO: &str = "google/protobuf/descriptor.proto";

/// A stand-in for angzarr's `options.proto`: the five extensions at their
/// numbers. Each is typed `bytes`, which has the same wire form as the
/// option messages, so it defines the numbers without the messages.
fn options_file() -> FileProto {
    let ext = |name: &str, number: u32, extendee: &str, label: i32| FieldProto {
        name: Some(name.into()),
        extendee: Some(extendee.into()),
        number: Some(number as i32),
        label: Some(label),
        r#type: Some(ty::BYTES),
        type_name: None,
        options: None,
        oneof_index: None,
        proto3_optional: None,
    };
    let msg = ".google.protobuf.MessageOptions";
    FileProto {
        name: Some(OPTIONS_PROTO.into()),
        package: Some("io.angzarr.v1".into()),
        dependency: vec![DESCRIPTOR_PROTO.into()],
        message_type: Vec::new(),
        enum_type: Vec::new(),
        extension: vec![
            ext("component", COMPONENT, msg, LABEL_OPTIONAL),
            ext("command", COMMAND, msg, LABEL_OPTIONAL),
            ext("event", EVENT, msg, LABEL_REPEATED),
            ext("state", STATE, msg, LABEL_OPTIONAL),
            ext(
                "conflict",
                CONFLICT,
                ".google.protobuf.FieldOptions",
                LABEL_OPTIONAL,
            ),
        ],
        syntax: Some("proto3".into()),
        buf: Some(BufExt {
            is_import: Some(true),
        }),
    }
}

/// A descriptor set: the fixture files, and (unless [`Image::bare`]) the
/// options file and `descriptor.proto` as imports.
#[derive(Clone, Debug)]
pub struct Image {
    files: Vec<(File, bool)>,
    with_options: bool,
}

impl Default for Image {
    fn default() -> Image {
        Image::new()
    }
}

impl Image {
    /// An image that defines the angzarr options.
    pub fn new() -> Image {
        Image {
            files: Vec::new(),
            with_options: true,
        }
    }

    /// An image without `options.proto`: option bytes it carries are
    /// defined by nothing in it.
    pub fn bare() -> Image {
        Image {
            files: Vec::new(),
            with_options: false,
        }
    }

    /// A file generated in this run.
    pub fn file(mut self, f: File) -> Image {
        self.files.push((f, false));
        self
    }

    /// A file present only as an import.
    pub fn import(mut self, f: File) -> Image {
        self.files.push((f, true));
        self
    }

    /// The encoded `FileDescriptorSet`, buf-image style: an import carries
    /// `ImageFileExtension.is_import`.
    pub fn encode(&self) -> Vec<u8> {
        let mut set = FileSetProto { file: Vec::new() };
        if self.with_options {
            let descriptor = DescriptorPool::global()
                .get_file_by_name(DESCRIPTOR_PROTO)
                .expect("the well-known types include descriptor.proto");
            let mut descriptor_bytes = descriptor.encode_to_vec();
            descriptor_bytes.extend(raw_option(
                8042,
                &BufExt {
                    is_import: Some(true),
                }
                .encode_to_vec(),
            ));
            set.file.push(descriptor_bytes);
            set.file.push(options_file().encode_to_vec());
        }
        for (f, is_import) in &self.files {
            set.file.push(f.proto(*is_import).encode_to_vec());
        }
        set.encode_to_vec()
    }

    /// The image as the core reads it.
    pub fn set(&self) -> DescriptorSet {
        let pool = DescriptorPool::decode(self.encode().as_slice())
            .expect("a fixture image is a valid descriptor set");
        let generated: BTreeSet<String> = self
            .files
            .iter()
            .filter(|(_, is_import)| !is_import)
            .map(|(f, _)| f.name.clone())
            .collect();
        DescriptorSet { pool, generated }
    }
}

/// `(component)` options of `kind` with `domain`, `input_domain` and
/// `output_domains` (empty strings and lists for unset).
pub fn component(
    kind: ComponentKind,
    domain: &str,
    input: &str,
    outputs: &[&str],
) -> ComponentOptions {
    ComponentOptions {
        kind: kind as i32,
        domain: domain.into(),
        input_domain: input.into(),
        output_domains: outputs.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

/// `(command)` handled by `component`, emitting `emits`.
pub fn command(component: &str, emits: &[&str]) -> CommandOptions {
    CommandOptions {
        component: component.into(),
        emits: emits.iter().map(|s| s.to_string()).collect(),
    }
}

/// One `(event)` entry.
pub fn event(component: &str, domain: &str, applies: bool) -> EventConsumer {
    EventConsumer {
        component: component.into(),
        domain: domain.into(),
        applies,
    }
}

/// `(state)` listing `resolutions`.
pub fn state(resolutions: &[ConflictResolution]) -> StateOptions {
    StateOptions {
        conflict_resolutions: resolutions.iter().map(|r| *r as i32).collect(),
    }
}

/// A well-formed counter: an aggregate in domain `counter` with one command
/// that emits one event it applies, in `counter.proto`, package `c.v1`.
pub fn counter() -> File {
    File::new("counter.proto", "c.v1")
        .message(
            Msg::new("CounterState")
                .component(component(ComponentKind::Aggregate, "counter", "", &[]))
                .field(Fld::scalar("value", 1, ty::INT64)),
        )
        .message(Msg::new("Increment").command(command("c.v1.CounterState", &["c.v1.Incremented"])))
        .message(Msg::new("Incremented").event(event("c.v1.CounterState", "", false)))
}
