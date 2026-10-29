//! `KEY=VALUE` pairs: a JSON object in Payloads, repeated `--flag KEY=VALUE` on the command line.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

/// One `KEY=VALUE` pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValue {
    /// The key.
    pub key: String,
    /// The value.
    pub value: String,
}

impl KeyValue {
    /// A pair from anything string-like.
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

impl std::str::FromStr for KeyValue {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (key, value) = s
            .split_once('=')
            .or_else(|| s.split_once(':'))
            .ok_or_else(|| format!("expected KEY=VALUE, got `{s}`"))?;
        Ok(Self::new(key.trim(), value.trim()))
    }
}

/// Collects pairs into a map (later keys win).
pub fn to_map(pairs: &[KeyValue]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|kv| (kv.key.clone(), kv.value.clone()))
        .collect()
}

/// `serde(with = "crate::kv::as_map")`: (de)serializes `Vec<KeyValue>` as a JSON object.
pub mod as_map {
    use super::*;

    /// Serializes the pairs as a map.
    pub fn serialize<S: Serializer>(pairs: &[KeyValue], s: S) -> Result<S::Ok, S::Error> {
        to_map(pairs).serialize(s)
    }

    /// Deserializes a map into pairs.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<KeyValue>, D::Error> {
        let map = BTreeMap::<String, String>::deserialize(d)?;
        Ok(map.into_iter().map(|(k, v)| KeyValue::new(k, v)).collect())
    }
}
