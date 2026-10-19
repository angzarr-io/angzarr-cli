//! The `io.angzarr.v1` options, read by field number from a message's or a
//! field's raw option bytes into angzarr-proto's types.
//!
//! Matching is by number, never by full name, so a descriptor set that does
//! not itself define the extensions still decodes; whether it does is a lint
//! of its own.

use std::collections::BTreeSet;

use angzarr_proto::v1::{
    CommandOptions, ComponentOptions, ConflictUnit, EventConsumer, StateOptions,
};
use prost::Message;
use prost::encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field};

/// `(io.angzarr.v1.component)` on `google.protobuf.MessageOptions`.
pub const COMPONENT: u32 = 50100;
/// `(io.angzarr.v1.command)` on `google.protobuf.MessageOptions`.
pub const COMMAND: u32 = 50104;
/// `(io.angzarr.v1.event)`, repeated, on `google.protobuf.MessageOptions`.
pub const EVENT: u32 = 50105;
/// `(io.angzarr.v1.state)` on `google.protobuf.MessageOptions`.
pub const STATE: u32 = 50106;
/// `(io.angzarr.v1.conflict)` on `google.protobuf.FieldOptions`.
pub const CONFLICT: u32 = 50107;

/// The message options angzarr defines.
pub const MESSAGE_OPTIONS: [u32; 4] = [COMPONENT, COMMAND, EVENT, STATE];

/// The angzarr options one message carries.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MessageOpts {
    pub component: Option<ComponentOptions>,
    pub command: Option<CommandOptions>,
    /// One entry per consuming component.
    pub events: Vec<EventConsumer>,
    pub state: Option<StateOptions>,
    /// Every angzarr option number present, decoded or not.
    pub present: BTreeSet<u32>,
    /// The angzarr option numbers whose bytes did not decode.
    pub malformed: BTreeSet<u32>,
}

/// The angzarr option one field carries.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldOpts {
    pub conflict: Option<ConflictUnit>,
    /// Whether `(conflict)` is present, decoded or not.
    pub present: bool,
    /// Whether its bytes did not decode.
    pub malformed: bool,
}

/// One length-delimited field of an options message, by number.
struct Raw<'a> {
    number: u32,
    bytes: Option<&'a [u8]>,
}

/// The fields of an encoded options message, in order. A field that is not
/// length-delimited carries no bytes; bytes that end mid-field stop the walk
/// and are reported as `Err` with the number reached, when known.
fn fields(mut buf: &[u8]) -> (Vec<Raw<'_>>, Option<Option<u32>>) {
    let mut out = Vec::new();
    while !buf.is_empty() {
        let Ok((number, wire)) = decode_key(&mut buf) else {
            return (out, Some(None));
        };
        if wire == WireType::LengthDelimited {
            let Ok(len) = decode_varint(&mut buf) else {
                return (out, Some(Some(number)));
            };
            let Ok(len) = usize::try_from(len) else {
                return (out, Some(Some(number)));
            };
            if len > buf.len() {
                return (out, Some(Some(number)));
            }
            out.push(Raw {
                number,
                bytes: Some(&buf[..len]),
            });
            buf = &buf[len..];
        } else {
            if skip_field(wire, number, &mut buf, DecodeContext::default()).is_err() {
                return (out, Some(Some(number)));
            }
            out.push(Raw {
                number,
                bytes: None,
            });
        }
    }
    (out, None)
}

/// Merges one occurrence into a singular message option, as protobuf does
/// for a field given twice.
fn merge<M: Message + Default>(slot: &mut Option<M>, bytes: Option<&[u8]>) -> bool {
    let Some(bytes) = bytes else { return false };
    slot.get_or_insert_with(M::default).merge(bytes).is_ok()
}

/// Reads the angzarr options from an encoded `MessageOptions`.
pub fn message_opts(raw: &[u8]) -> MessageOpts {
    let mut opts = MessageOpts::default();
    let (raw_fields, truncated) = fields(raw);
    for field in raw_fields {
        let ok = match field.number {
            COMPONENT => merge(&mut opts.component, field.bytes),
            COMMAND => merge(&mut opts.command, field.bytes),
            STATE => merge(&mut opts.state, field.bytes),
            EVENT => match field.bytes.map(EventConsumer::decode) {
                Some(Ok(event)) => {
                    opts.events.push(event);
                    true
                }
                _ => false,
            },
            _ => continue,
        };
        opts.present.insert(field.number);
        if !ok {
            opts.malformed.insert(field.number);
        }
    }
    if let Some(Some(number)) = truncated
        && MESSAGE_OPTIONS.contains(&number)
    {
        opts.present.insert(number);
        opts.malformed.insert(number);
    }
    opts
}

/// Reads the angzarr option from an encoded `FieldOptions`.
pub fn field_opts(raw: &[u8]) -> FieldOpts {
    let mut opts = FieldOpts::default();
    let (raw_fields, truncated) = fields(raw);
    for field in raw_fields.into_iter().filter(|f| f.number == CONFLICT) {
        opts.present = true;
        if !merge(&mut opts.conflict, field.bytes) {
            opts.malformed = true;
        }
    }
    if truncated == Some(Some(CONFLICT)) {
        opts.present = true;
        opts.malformed = true;
    }
    opts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{option, varint_option};

    #[test]
    fn each_message_option_decodes_by_number() {
        let component = ComponentOptions {
            kind: 1,
            domain: "orders".into(),
            ..Default::default()
        };
        let command = CommandOptions {
            component: "o.v1.OrderState".into(),
            emits: vec!["o.v1.Placed".into()],
        };
        let e1 = EventConsumer {
            component: "o.v1.OrderState".into(),
            ..Default::default()
        };
        let e2 = EventConsumer {
            component: "o.v1.Shipping".into(),
            domain: "orders".into(),
            applies: false,
        };
        let state = StateOptions {
            conflict_resolutions: vec![1],
        };
        let mut raw = varint_option(7, 1);
        raw.extend(option(COMPONENT, &component));
        raw.extend(option(COMMAND, &command));
        raw.extend(option(EVENT, &e1));
        raw.extend(option(EVENT, &e2));
        raw.extend(option(STATE, &state));
        let opts = message_opts(&raw);
        assert_eq!(opts.component, Some(component));
        assert_eq!(opts.command, Some(command));
        assert_eq!(opts.events, vec![e1, e2]);
        assert_eq!(opts.state, Some(state));
        assert_eq!(
            opts.present,
            BTreeSet::from([COMPONENT, COMMAND, EVENT, STATE])
        );
        assert!(opts.malformed.is_empty());
    }

    #[test]
    fn no_angzarr_option_is_nothing_present() {
        let opts = message_opts(&varint_option(7, 1));
        assert_eq!(opts, MessageOpts::default());
    }

    #[test]
    fn bytes_that_do_not_decode_are_malformed_not_a_panic() {
        // A length-delimited COMPONENT whose content is a truncated varint.
        let mut raw = Vec::new();
        prost::encoding::encode_key(COMPONENT, WireType::LengthDelimited, &mut raw);
        raw.extend([1, 0x80]);
        let opts = message_opts(&raw);
        assert!(opts.present.contains(&COMPONENT));
        assert!(opts.malformed.contains(&COMPONENT));

        // A COMMAND whose length runs past the end.
        let mut raw = Vec::new();
        prost::encoding::encode_key(COMMAND, WireType::LengthDelimited, &mut raw);
        raw.extend([9, 1]);
        let opts = message_opts(&raw);
        assert!(opts.malformed.contains(&COMMAND));
    }

    #[test]
    fn the_conflict_field_option_decodes_by_number() {
        let unit = ConflictUnit {
            descend: true,
            ..Default::default()
        };
        let opts = field_opts(&option(CONFLICT, &unit));
        assert_eq!(opts.conflict, Some(unit));
        assert!(opts.present && !opts.malformed);

        let empty = field_opts(&option(CONFLICT, &ConflictUnit::default()));
        assert!(empty.present);
        assert_eq!(empty.conflict, Some(ConflictUnit::default()));

        assert!(!field_opts(&varint_option(3, 1)).present);
    }
}
