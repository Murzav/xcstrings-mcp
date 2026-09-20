use std::fmt;

use serde::de::{DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

/// Original property order and explicit nulls, without a duplicate catalog tree.
#[derive(Debug, Clone, Default)]
pub struct ObjectLayout {
    fields: Vec<(String, bool)>,
}

impl ObjectLayout {
    pub(super) fn capture(fields: &Map<String, Value>) -> Self {
        Self {
            fields: fields
                .iter()
                .map(|(k, v)| (k.clone(), v.is_null()))
                .collect(),
        }
    }

    pub(super) fn contains(&self, name: &str) -> bool {
        self.fields.iter().any(|(key, _)| key == name)
    }

    pub(super) fn is_null(&self, name: &str) -> bool {
        self.fields.iter().any(|(key, null)| key == name && *null)
    }

    /// Forget explicit presence/null when deliberately removing a known field.
    pub fn forget(&mut self, name: &str) {
        self.fields.retain(|(key, _)| key != name);
    }

    pub(super) fn arrange(&self, mut fields: Map<String, Value>) -> Map<String, Value> {
        let mut ordered = Map::new();
        for (name, _) in &self.fields {
            if let Some(value) = fields.shift_remove(name) {
                ordered.insert(name.clone(), value);
            }
        }
        ordered.extend(fields);
        ordered
    }
}

pub(super) fn required<T: DeserializeOwned>(
    fields: &mut Map<String, Value>,
    name: &str,
) -> Result<T, String> {
    let value = fields
        .shift_remove(name)
        .ok_or_else(|| format!("missing field `{name}`"))?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

pub(super) fn optional<T: DeserializeOwned>(
    fields: &mut Map<String, Value>,
    name: &str,
) -> Result<Option<T>, String> {
    match fields.shift_remove(name) {
        Some(value) => serde_json::from_value(value).map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

pub(super) fn default_true(fields: &mut Map<String, Value>, name: &str) -> Result<bool, String> {
    match fields.shift_remove(name) {
        Some(value) => serde_json::from_value(value).map_err(|e| e.to_string()),
        None => Ok(true),
    }
}

/// Reject duplicate members at every physical level, including future metadata.
/// The temporary Value is consumed while constructing the typed catalog.
pub(crate) struct UniqueValue(pub Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON value with unique object members")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| UniqueValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(v.to_owned())))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(v)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                self.visit_unit()
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(UniqueValue(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(UniqueValue(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some(name) = map.next_key::<String>()? {
                    if values.contains_key(&name) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON member '{name}'"
                        )));
                    }
                    let UniqueValue(value) = map.next_value()?;
                    values.insert(name, value);
                }
                Ok(UniqueValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

macro_rules! catalog_object {
    ($name:ident { $($(#[$field_meta:meta])* $field:ident : $ty:ty = $default:expr => $wire:literal, $mode:ident;)* }) => {
        #[derive(Debug, Clone, schemars::JsonSchema)]
        #[serde(rename_all = "camelCase")]
        pub struct $name {
            $($(#[$field_meta])* pub $field: $ty,)*
            #[schemars(skip)]
            pub extra: OrderedMap<String, serde_json::Value>,
            #[schemars(skip)]
            pub layout: ObjectLayout,
        }
        impl Default for $name {
            fn default() -> Self { Self { $($field: $default,)* extra: OrderedMap::new(), layout: ObjectLayout::default() } }
        }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let layout::UniqueValue(value) = serde::Deserialize::deserialize(deserializer)?;
                let serde_json::Value::Object(mut fields) = value else {
                    return Err(serde::de::Error::custom(concat!(stringify!($name), " must be an object")));
                };
                let layout = ObjectLayout::capture(&fields);
                $(let $field = layout::$mode(&mut fields, $wire).map_err(serde::de::Error::custom)?;)*
                Ok(Self { $($field,)* extra: fields.into_iter().collect(), layout })
            }
        }
        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut fields = serde_json::Map::new();
                $(catalog_object!(@field fields self $field $wire $mode);)*
                for (name, value) in &self.extra {
                    if matches!(name.as_str(), $($wire)|*) {
                        return Err(serde::ser::Error::custom(format!("extra field '{name}' conflicts with a known field")));
                    }
                    // Serialization needs owned Values; persistent metadata stays single-copy.
                    fields.insert(name.clone(), value.clone());
                }
                self.layout.arrange(fields).serialize(serializer)
            }
        }
    };
    (@field $fields:ident $self:ident $field:ident $wire:literal required) => {
        $fields.insert($wire.into(), serde_json::to_value(&$self.$field).map_err(serde::ser::Error::custom)?);
    };
    (@field $fields:ident $self:ident $field:ident $wire:literal optional) => {
        if $self.$field.is_some() || $self.layout.is_null($wire) {
            $fields.insert($wire.into(), serde_json::to_value(&$self.$field).map_err(serde::ser::Error::custom)?);
        }
    };
    (@field $fields:ident $self:ident $field:ident $wire:literal default_true) => {
        if !$self.$field || $self.layout.contains($wire) { $fields.insert($wire.into(), $self.$field.into()); }
    };
}
pub(super) use catalog_object;
