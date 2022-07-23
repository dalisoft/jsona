use super::node::{
    ArrayInner, BoolInner, Node, NumberInner, NumberRepr, ObjectInner, StringInner, StringRepr,
};
use crate::dom::node::Key;
use serde::{
    de::Visitor,
    ser::{SerializeMap, SerializeSeq},
    Deserialize, Serialize, Serializer,
};
use serde_json::Number as JsonNumber;

impl Serialize for Node {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Node::Object(v) => {
                let properties = v.value().read();
                let mut map = ser.serialize_map(Some(properties.all.len()))?;

                for (key, property) in properties.all.iter() {
                    map.serialize_entry(key.value(), property)?;
                }

                map.end()
            }
            Node::Array(arr) => {
                let items = arr.inner.items.read();
                let mut seq = ser.serialize_seq(Some(items.len()))?;
                for item in &**items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Node::Bool(v) => ser.serialize_bool(v.value()),
            Node::String(v) => ser.serialize_str(v.value()),
            Node::Number(v) => v.value().serialize(ser),
            Node::Null(_) => ser.serialize_unit(),
        }
    }
}

#[derive(Default)]
struct JsonaVisitor;

impl<'de> Visitor<'de> for JsonaVisitor {
    type Value = Node;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "a JSONA value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BoolInner {
            errors: Default::default(),
            syntax: None,
            node_syntax: None,
            annotations: None,
            value: v.into(),
        }
        .wrap()
        .into())
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(NumberInner {
            errors: Default::default(),
            syntax: None,
            node_syntax: None,
            annotations: None,
            repr: NumberRepr::Dec,
            value: JsonNumber::from(v).into(),
        }
        .wrap()
        .into())
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(NumberInner {
            errors: Default::default(),
            syntax: None,
            node_syntax: None,
            annotations: None,
            repr: NumberRepr::Dec,
            value: JsonNumber::from(v).into(),
        }
        .wrap()
        .into())
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let value = match JsonNumber::from_f64(v) {
            Some(n) => n,
            None => {
                return Err(serde::de::Error::invalid_value(
                    serde::de::Unexpected::Float(v),
                    &self,
                ))
            }
        };
        Ok(NumberInner {
            errors: Default::default(),
            syntax: None,
            node_syntax: None,
            annotations: None,
            repr: NumberRepr::Dec,
            value: value.into(),
        }
        .wrap()
        .into())
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StringInner {
            errors: Default::default(),
            syntax: None,
            node_syntax: None,
            annotations: None,
            repr: StringRepr::Double,
            value: v.to_string().into(),
        }
        .wrap()
        .into())
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let _ = v;
        Err(serde::de::Error::invalid_type(
            serde::de::Unexpected::Bytes(v),
            &self,
        ))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Err(serde::de::Error::invalid_type(
            serde::de::Unexpected::Option,
            &self,
        ))
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Err(serde::de::Error::invalid_type(
            serde::de::Unexpected::Unit,
            &self,
        ))
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let array = ArrayInner {
            errors: Default::default(),
            syntax: None,
            node_syntax: None,
            annotations: None,
            items: Default::default(),
        };

        array.items.update(|items| loop {
            match seq.next_element::<Node>() {
                Ok(Some(node)) => {
                    items.push(node);
                }
                Ok(None) => break,
                Err(_) => {}
            }
        });

        Ok(array.wrap().into())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let object = ObjectInner {
            errors: Default::default(),
            syntax: None,
            node_syntax: None,
            annotations: None,
            properties: Default::default(),
        };

        object.properties.update(|entries| loop {
            match map.next_entry::<String, Node>() {
                Ok(Some((key, node))) => {
                    entries.add(Key::property(key), node);
                }
                Ok(None) => break,
                Err(_) => {}
            }
        });

        Ok(object.wrap().into())
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::EnumAccess<'de>,
    {
        let _ = data;
        Err(serde::de::Error::invalid_type(
            serde::de::Unexpected::Enum,
            &self,
        ))
    }
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        de.deserialize_any(JsonaVisitor::default())
    }
}