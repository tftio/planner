//! Repo-owned YAML adapter.
//!
//! Every YAML decode and encode in the crate goes through this module, so the
//! third-party YAML library is named in exactly one place (ENG-010). The library
//! is `serde-saphyr`, which is written in safe Rust. Decoding accepts only
//! `true` and `false` as booleans, and an untyped scalar with leading zeros or
//! underscores stays a string rather than becoming a number. Error messages are
//! rendered without a source snippet, because the caller attaches its own
//! location to the diagnostic.

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, Serializer};
use thiserror::Error;

/// YAML text that is not YAML of the expected shape.
#[derive(Debug, Clone, Eq, PartialEq, Error)]
#[error("{message}")]
pub struct DecodeError {
    message: String,
    offset: usize,
}

impl DecodeError {
    /// Byte offset of the failure within the decoded text, or 0 when the
    /// library reports no position.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }
}

/// A value that could not be rendered as YAML.
#[derive(Debug, Clone, Eq, PartialEq, Error)]
#[error("{0}")]
pub struct EncodeError(String);

/// An untyped YAML value, for the places that must carry YAML they do not model.
///
/// Mapping entries keep their source order so a decode followed by an encode
/// does not reorder an author's keys.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    Integer(i128),
    Float(f64),
    String(String),
    Sequence(Vec<Self>),
    Mapping(Vec<(Self, Self)>),
}

/// Decode YAML text into `T`.
pub(crate) fn from_str<'a, T>(text: &'a str) -> Result<T, DecodeError>
where
    T: Deserialize<'a>,
{
    let options = serde_saphyr::options! {
        strict_booleans: true,
        leading_zero_numbers_as_strings: true,
        underscored_numbers_as_strings: true,
        with_snippet: false,
    };
    serde_saphyr::from_str_with_options(text, options).map_err(|error| {
        let offset = error
            .location()
            .and_then(|location| location.span().byte_offset())
            .and_then(|offset| usize::try_from(offset).ok())
            .unwrap_or(0);
        DecodeError {
            message: error.render_with_formatter(&serde_saphyr::UserMessageFormatter),
            offset,
        }
    })
}

/// Encode `value` as block-style YAML ending in a newline.
///
/// A string with a newline is written as a `|` literal block, and a long string
/// without one stays on one line rather than being folded at 80 columns. Quoting
/// follows YAML 1.2, so a date such as `2026-09-10` is written unquoted; the
/// `%YAML 1.2` directive that would normally announce this is left out, because
/// the output is embedded in Markdown frontmatter and fences.
pub(crate) fn to_string<T>(value: &T) -> Result<String, EncodeError>
where
    T: Serialize,
{
    let options = serde_saphyr::ser_options! {
        folded_wrap_chars: usize::MAX,
        yaml_12: true,
        no_lang_directive: true,
    };
    serde_saphyr::to_string_with_options(value, options)
        .map_err(|error| EncodeError(error.to_string()))
}

impl Value {
    /// The `tasks` sequence of a task-graph mapping.
    ///
    /// # Errors
    ///
    /// Names the shape the value lacks: a mapping, or a `tasks` list inside it.
    pub(crate) fn tasks_mut(&mut self) -> Result<&mut Vec<Self>, &'static str> {
        let Self::Mapping(entries) = self else {
            return Err("task graph YAML must be a mapping");
        };
        entries
            .iter_mut()
            .find_map(|(key, value)| match (key, value) {
                (Self::String(name), Self::Sequence(tasks)) if name == "tasks" => Some(tasks),
                _ => None,
            })
            .ok_or("task graph YAML must contain a tasks list")
    }

    /// Remove every entry whose key is the string `key`; non-mappings are left alone.
    pub(crate) fn remove_key(&mut self, key: &str) {
        if let Self::Mapping(entries) = self {
            entries.retain(|(name, _)| !matches!(name, Self::String(name) if name == key));
        }
    }

    /// A deterministic one-line text form, used where a list item the model
    /// stores as text was written as structured YAML.
    pub(crate) fn into_text(self) -> String {
        match self {
            Self::Null => "null".to_owned(),
            Self::Bool(value) => value.to_string(),
            Self::Integer(value) => value.to_string(),
            Self::Float(value) => float_text(value),
            Self::String(value) => value,
            Self::Sequence(values) => format!(
                "[{}]",
                values
                    .into_iter()
                    .map(Self::into_text)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Mapping(entries) => entries
                .into_iter()
                .map(|(key, value)| format!("{}: {}", key.into_text(), value.into_text()))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

/// YAML 1.2 spellings for the non-finite floats; the shortest round-trip
/// decimal for the rest, always with a fractional part or exponent.
fn float_text(value: f64) -> String {
    if value.is_nan() {
        ".nan".to_owned()
    } else if value.is_infinite() {
        if value.is_sign_positive() {
            ".inf".to_owned()
        } else {
            "-.inf".to_owned()
        }
    } else {
        format!("{value:?}")
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Integer(value) => serializer.serialize_i128(*value),
            Self::Float(value) => serializer.serialize_f64(*value),
            Self::String(value) => serializer.serialize_str(value),
            Self::Sequence(values) => serializer.collect_seq(values),
            Self::Mapping(entries) => {
                serializer.collect_map(entries.iter().map(|(key, value)| (key, value)))
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any YAML value")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Integer(i128::from(value)))
    }

    fn visit_i128<E: de::Error>(self, value: i128) -> Result<Value, E> {
        Ok(Value::Integer(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Integer(i128::from(value)))
    }

    fn visit_u128<E: de::Error>(self, value: u128) -> Result<Value, E> {
        i128::try_from(value)
            .map(Value::Integer)
            .map_err(|_| E::custom("integer is out of range"))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        Ok(Value::Float(value))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_seq<A>(self, mut access: A) -> Result<Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = access.next_element()? {
            values.push(value);
        }
        Ok(Value::Sequence(values))
    }

    fn visit_map<A>(self, mut access: A) -> Result<Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut entries = Vec::new();
        while let Some(entry) = access.next_entry()? {
            entries.push(entry);
        }
        Ok(Value::Mapping(entries))
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde::de::IntoDeserializer;
    use serde::de::value::{
        BytesDeserializer, Error as ValueError, I128Deserializer, U128Deserializer,
    };

    use super::{Value, from_str, to_string};

    #[test]
    fn decode_failure_reports_message_and_byte_offset() {
        let error = from_str::<Vec<u32>>("- 1\n- two\n").err();
        let offset = error.as_ref().map(super::DecodeError::offset);
        let message = error.map(|error| error.to_string()).unwrap_or_default();

        assert_eq!(offset, Some(6));
        assert!(message.contains("line 2"), "{message}");
    }

    #[test]
    fn decode_keeps_yaml_1_1_booleans_and_padded_numbers_as_strings() {
        let values = from_str::<Value>("[yes, off, 0755, 1_000, true]").ok();

        assert_eq!(
            values,
            Some(Value::Sequence(vec![
                Value::String("yes".to_owned()),
                Value::String("off".to_owned()),
                Value::String("0755".to_owned()),
                Value::String("1_000".to_owned()),
                Value::Bool(true),
            ]))
        );
    }

    #[test]
    fn encode_failure_is_reported() {
        struct Unrenderable;

        impl serde::Serialize for Unrenderable {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom("cannot render this value"))
            }
        }

        let message = to_string(&Unrenderable)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();

        assert!(message.contains("cannot render this value"), "{message}");
    }

    #[test]
    fn encode_keeps_dates_plain_long_lines_whole_and_newlines_literal() {
        let long = "word ".repeat(30).trim_end().to_owned();
        let value = Value::Mapping(vec![
            (
                Value::String("date".to_owned()),
                Value::String("2026-09-10".to_owned()),
            ),
            (
                Value::String("long".to_owned()),
                Value::String(long.clone()),
            ),
            (
                Value::String("text".to_owned()),
                Value::String("one\ntwo\n".to_owned()),
            ),
            (
                Value::String("padded".to_owned()),
                Value::String("0755".to_owned()),
            ),
        ]);

        assert_eq!(
            to_string(&value).ok(),
            Some(format!(
                "date: 2026-09-10\nlong: {long}\ntext: |\n  one\n  two\npadded: \"0755\"\n"
            ))
        );
    }

    #[test]
    fn every_value_kind_survives_a_round_trip_in_key_order() {
        let value = Value::Mapping(vec![
            (Value::String("z".to_owned()), Value::Null),
            (Value::String("b".to_owned()), Value::Bool(false)),
            (Value::String("i".to_owned()), Value::Integer(-7)),
            (Value::String("f".to_owned()), Value::Float(1.5)),
            (
                Value::String("s".to_owned()),
                Value::Sequence(vec![Value::String("yes".to_owned())]),
            ),
        ]);

        let rendered = to_string(&value).unwrap_or_default();
        assert_eq!(rendered, "z: null\nb: false\ni: -7\nf: 1.5\ns:\n- yes\n");
        assert_eq!(from_str::<Value>(&rendered).ok(), Some(value));
    }

    #[test]
    fn wide_integers_decode_and_out_of_range_ones_are_rejected() {
        let wide = Value::deserialize(I128Deserializer::<ValueError>::new(-1 << 100)).ok();
        let unsigned = Value::deserialize(U128Deserializer::<ValueError>::new(1 << 100)).ok();
        let too_wide = Value::deserialize(U128Deserializer::<ValueError>::new(u128::MAX))
            .err()
            .map(|error| error.to_string());
        let owned = Value::deserialize(IntoDeserializer::<ValueError>::into_deserializer(
            "owned".to_owned(),
        ))
        .ok();
        let large =
            Value::deserialize(IntoDeserializer::<ValueError>::into_deserializer(u64::MAX)).ok();

        assert_eq!(wide, Some(Value::Integer(-1 << 100)));
        assert_eq!(unsigned, Some(Value::Integer(1 << 100)));
        assert_eq!(too_wide.as_deref(), Some("integer is out of range"));
        assert_eq!(owned, Some(Value::String("owned".to_owned())));
        assert_eq!(large, Some(Value::Integer(i128::from(u64::MAX))));
    }

    #[test]
    fn non_yaml_input_names_what_was_expected() {
        let message = Value::deserialize(BytesDeserializer::<ValueError>::new(b"raw"))
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();

        assert!(message.contains("any YAML value"), "{message}");
    }

    #[test]
    fn floats_project_to_yaml_spellings() {
        let projected = [1.5, 2.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY]
            .map(|value| Value::Float(value).into_text());

        assert_eq!(projected, ["1.5", "2.0", ".nan", ".inf", "-.inf"]);
    }

    #[test]
    fn tasks_mut_finds_the_list_or_names_the_missing_shape() {
        let mut scalar = Value::String("tasks".to_owned());
        let mut no_tasks = Value::Mapping(vec![(
            Value::String("tasks".to_owned()),
            Value::String("not a list".to_owned()),
        )]);

        assert_eq!(
            scalar.tasks_mut().err(),
            Some("task graph YAML must be a mapping")
        );
        assert_eq!(
            no_tasks.tasks_mut().err(),
            Some("task graph YAML must contain a tasks list")
        );

        let mut graph = Value::Mapping(vec![(
            Value::String("tasks".to_owned()),
            Value::Sequence(vec![Value::Null]),
        )]);
        assert_eq!(graph.tasks_mut().ok(), Some(&mut vec![Value::Null]));
    }

    #[test]
    fn remove_key_drops_matching_entries_and_leaves_non_mappings_alone() {
        let mut task = Value::Mapping(vec![
            (
                Value::String("id".to_owned()),
                Value::String("T001".to_owned()),
            ),
            (
                Value::String("hidden_criteria".to_owned()),
                Value::Sequence(vec![]),
            ),
        ]);
        let mut scalar = Value::String("hidden_criteria".to_owned());
        task.remove_key("hidden_criteria");
        scalar.remove_key("hidden_criteria");

        assert_eq!(
            task,
            Value::Mapping(vec![(
                Value::String("id".to_owned()),
                Value::String("T001".to_owned())
            )])
        );
        assert_eq!(scalar, Value::String("hidden_criteria".to_owned()));
    }
}
