use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use std::num::ParseIntError;
use std::str::FromStr;

#[derive(Serialize, Clone)]
struct NBTMapEntry<T: Serialize> {
    name: String,
    id: i32,
    element: T,
}

/// This is a format used in the current network protocol,
/// most notably used in the JoinGame packet.
#[derive(Serialize, Clone)]
pub struct NBTMap<T: Serialize> {
    #[serde(rename = "type")]
    self_type: String,
    value: Vec<NBTMapEntry<T>>,
}

impl<T: Serialize> NBTMap<T> {
    pub fn new(self_type: String) -> NBTMap<T> {
        NBTMap {
            self_type,
            value: Vec::new(),
        }
    }

    pub fn push_element(&mut self, name: String, element: T) {
        let id = self.value.len() as i32;
        self.value.push(NBTMapEntry { name, id, element });
    }
}

/// An easy way to create HashMaps
macro_rules! map(
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = ::std::collections::HashMap::new();
            $(
                m.insert($key, $value);
            )+
            m
        }
     };
);

macro_rules! nbt_unwrap_val {
    // I'm not sure if path is the right type here.
    // It works though!
    ($e:expr, $p:path) => {
        match $e {
            $p(val) => val,
            _ => return None,
        }
    };
}

pub struct HyphenatedUUID(pub u128);

impl ToString for HyphenatedUUID {
    fn to_string(&self) -> String {
        let mut hex = format!("{:032x}", self.0);
        hex.insert(8, '-');
        hex.insert(13, '-');
        hex.insert(18, '-');
        hex.insert(23, '-');
        hex
    }
}

impl FromStr for HyphenatedUUID {
    type Err = ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.replace('-', "");
        Ok(HyphenatedUUID(u128::from_str_radix(&hex, 16)?))
    }
}

impl Serialize for HyphenatedUUID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

struct HyphenatedUUIDVisitor;

impl<'de> Visitor<'de> for HyphenatedUUIDVisitor {
    type Value = HyphenatedUUID;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a hyphenated uuid string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse().map_err(E::custom)
    }
}

impl<'de> Deserialize<'de> for HyphenatedUUID {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(HyphenatedUUIDVisitor)
    }
}
