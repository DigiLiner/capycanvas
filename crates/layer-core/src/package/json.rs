use crate::authored::PortableId;
use serde::{Deserializer, de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor}};
use serde_json::{Map, Number, Value};
use std::{cell::Cell, collections::BTreeMap, fmt};
use super::values::{DecodeError, DecodeResult};

pub const MAX_JSON_DEPTH: usize = 128;
pub const MAX_JSON_NODES: usize = 4_194_304;
const NODE_LIMIT: &str = "Package JSON exceeds the node limit";

struct Unique<'a,const RETAIN:bool>(&'a Cell<usize>,usize);
impl<const RETAIN:bool> Unique<'_,RETAIN> {
    fn node<E: Error>(&self) -> Result<(), E> {
        let count = self.0.get() + 1;
        if count > self.1 { return Err(E::custom(NODE_LIMIT)); }
        self.0.set(count);
        Ok(())
    }
}
impl<'de,const RETAIN:bool> DeserializeSeed<'de> for Unique<'_,RETAIN> {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> { deserializer.deserialize_any(self) }
}
impl<'de,const RETAIN:bool> Visitor<'de> for Unique<'_,RETAIN> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("JSON without duplicate keys") }
    fn visit_bool<E: Error>(self, v: bool) -> Result<Value, E> { self.node()?; Ok(Value::Bool(v)) }
    fn visit_i64<E: Error>(self, v: i64) -> Result<Value, E> { self.node()?; Ok(Value::Number(v.into())) }
    fn visit_u64<E: Error>(self, v: u64) -> Result<Value, E> { self.node()?; Ok(Value::Number(v.into())) }
    fn visit_f64<E: Error>(self, v: f64) -> Result<Value, E> {
        self.node()?;
        Number::from_f64(v).map(Value::Number).ok_or_else(|| E::custom("Non-finite JSON number"))
    }
    fn visit_str<E: Error>(self, v: &str) -> Result<Value, E> { self.node()?; Ok(if RETAIN {Value::String(v.into())} else {Value::Null}) }
    fn visit_string<E: Error>(self, v: String) -> Result<Value, E> { self.node()?; Ok(if RETAIN {Value::String(v)} else {Value::Null}) }
    fn visit_unit<E: Error>(self) -> Result<Value, E> { self.node()?; Ok(Value::Null) }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        self.node()?;
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Unique::<RETAIN>(self.0,self.1))? { if RETAIN {values.push(value);} }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        self.node()?;
        let mut values = Map::new();
        if !RETAIN {
            while map.next_key::<serde::de::IgnoredAny>()?.is_some() {map.next_value_seed(Unique::<RETAIN>(self.0,self.1))?;}
            return Ok(Value::Null);
        }
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) { return Err(A::Error::custom("Duplicate JSON key")); }
            let value = map.next_value_seed(Unique::<RETAIN>(self.0,self.1))?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

pub fn parse_json(bytes: &[u8], limit: usize) -> DecodeResult<Value> {
    let mut remaining=MAX_JSON_NODES;
    parse_json_with_budget(bytes,limit,&mut remaining)
}

pub fn json_node_count(bytes:&[u8],limit:usize)->DecodeResult<usize> {
    let mut remaining=MAX_JSON_NODES;parse::<false>(bytes,limit,&mut remaining)?;Ok(MAX_JSON_NODES-remaining)
}
pub fn parse_json_with_budget(bytes:&[u8],limit:usize,remaining_nodes:&mut usize)->DecodeResult<Value> {
    parse::<true>(bytes,limit,remaining_nodes)
}
fn parse<const RETAIN:bool>(bytes:&[u8],limit:usize,remaining_nodes:&mut usize)->DecodeResult<Value> {
    if bytes.len() > limit { return Err(DecodeError::Unsupported("Package metadata exceeds limit".into())); }
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for byte in bytes {
        if quoted {
            if escaped { escaped = false; } else if *byte == b'\\' { escaped = true; } else if *byte == b'"' { quoted = false; }
        } else {
            match byte {
                b'"' => quoted = true,
                b'[' | b'{' => { depth += 1; if depth >= MAX_JSON_DEPTH { return Err(DecodeError::Unsupported("Package JSON depth exceeds limit".into())); } },
                b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {},
            }
        }
    }
    let nodes = Cell::new(0);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let allowance=(*remaining_nodes).min(MAX_JSON_NODES);
    let parsed = Unique::<RETAIN>(&nodes,allowance).deserialize(&mut deserializer).and_then(|value| deserializer.end().map(|()| value));
    *remaining_nodes=remaining_nodes.saturating_sub(nodes.get());
    parsed.map_err(|error| if nodes.get() >= allowance && error.to_string().starts_with(NODE_LIMIT) {
        DecodeError::Unsupported(NODE_LIMIT.into())
    } else { DecodeError::Invalid(error.to_string()) })
}

pub fn references(value: &Value, limit: usize) -> DecodeResult<Vec<PortableId>> {
    let mut pending = vec![(value, 0)];
    let mut result = Vec::new();
    let mut visited = 0usize;
    while let Some((value, depth)) = pending.pop() {
        visited = visited.checked_add(1).ok_or("Reference traversal overflow")?;
        if visited > limit || depth >= MAX_JSON_DEPTH || depth == MAX_JSON_DEPTH-1 && matches!(value,Value::Object(_)|Value::Array(_)) { return Err(DecodeError::Unsupported("Reference traversal exceeds limit".into())); }
        match value {
            Value::Object(fields) => {
                if let Some(reference) = fields.get("ref") {
                    if fields.len() != 1 { return Err("Invalid reference envelope".into()); }
                    result.push(reference.as_str().ok_or("Invalid reference identity")?.parse()?);
                } else {
                    if fields.len() > limit.saturating_sub(visited + pending.len()) { return Err(DecodeError::Unsupported("Reference traversal exceeds limit".into())); }
                    let ignored = fields.get("type").and_then(Value::as_str).and_then(super::registry::descriptor)
                        .map_or(&[][..], |record| record.ignored_reference_fields);
                    for (key,value) in fields {
                        if key == "data" && !ignored.is_empty() && let Some(data) = value.as_object() {
                            pending.extend(data.iter().filter(|(key,_)|!ignored.contains(&key.as_str())).map(|(_,value)|(value,depth+2)));
                        } else { pending.push((value,depth+1)); }
                    }
                }
            }
            Value::Array(values) => {
                if values.len() > limit.saturating_sub(visited + pending.len()) { return Err(DecodeError::Unsupported("Reference traversal exceeds limit".into())); }
                pending.extend(values.iter().map(|value| (value, depth + 1)));
            }
            _ => (),
        }
    }
    Ok(result)
}

pub fn remap_references(value: &mut Value, identities: &BTreeMap<PortableId, PortableId>, limit: usize) -> DecodeResult<()> {
    references(value, limit)?;
    fn remap(value: &mut Value, identities: &BTreeMap<PortableId, PortableId>) {
        match value {
            Value::Object(fields) => {
                if let Some(reference) = fields.get_mut("ref") {
                    if let Some(replacement) = reference.as_str().and_then(|s| s.parse().ok()).and_then(|id| identities.get(&id)) {
                        *reference = Value::String(replacement.to_string());
                    }
                } else { for value in fields.values_mut() { remap(value, identities); } }
            }
            Value::Array(values) => { for value in values { remap(value, identities); } }
            _ => (),
        }
    }
    remap(value, identities);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn related_json_inputs_share_one_node_allowance() {
        let mut remaining=5;
        assert_eq!(parse_json_with_budget(b"[1,2]",128,&mut remaining).unwrap(),serde_json::json!([1,2]));
        assert_eq!(remaining,2);
        assert!(parse_json_with_budget(b"[3,4]",128,&mut remaining).is_err());
        assert_eq!(remaining,0);
        assert!(parse_json_with_budget(b"null",128,&mut remaining).is_err());
    }

    #[test]
    fn counted_json_uses_the_same_value_node_and_depth_limits() {
        let bytes=br#"{"a":[1,2],"b":"value"}"#;
        assert_eq!(json_node_count(bytes,128).unwrap(),5);
        let mut remaining=4;assert!(parse::<false>(bytes,128,&mut remaining).is_err());
        assert_eq!(remaining,0);
        assert!(json_node_count(b"[1] trailing",128).is_err());
        assert!(json_node_count(&[vec![b'[';128],vec![b']';128]].concat(),1024).is_err());
    }

    #[test]
    fn finite_double_values_round_trip_bitwise() {
        let mut bits = 0x4a5f_038c_9172_db69u64;
        for _ in 0..100_000 {
            bits ^= bits << 13;
            bits ^= bits >> 7;
            bits ^= bits << 17;
            let value = f64::from_bits(bits);
            if !value.is_finite() { continue; }
            let bytes = serde_json::to_vec(&value).unwrap();
            let restored = parse_json(&bytes, 128).unwrap().as_f64().unwrap();
            assert_eq!(restored.to_bits(), value.to_bits());
        }
        for value in [-0., f64::MIN_POSITIVE, f64::from_bits(1), f64::MAX, 16_777_217.125] {
            let bytes = serde_json::to_vec(&value).unwrap();
            assert_eq!(parse_json(&bytes, 128).unwrap().as_f64().unwrap().to_bits(), value.to_bits());
        }
    }
}
