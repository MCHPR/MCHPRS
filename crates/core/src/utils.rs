use mchprs_blocks::items::ItemStack;
use mchprs_network::packets::SlotData;
use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use std::num::ParseIntError;
use std::str::FromStr;

#[derive(Debug)]
pub struct HyphenatedUUID(pub u128);

impl std::fmt::Display for HyphenatedUUID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            self.0 >> 96,
            (self.0 >> 80) & 0xFFFF,
            (self.0 >> 64) & 0xFFFF,
            (self.0 >> 48) & 0xFFFF,
            self.0 & 0xFFFFFFFFFFFF,
        )
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

impl Visitor<'_> for HyphenatedUUIDVisitor {
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

pub fn encode_slot_data(item: &ItemStack) -> SlotData {
    SlotData {
        item_count: item.count as i32,
        item_id: item.item_type.get_id() as i32,
        container_items: item
            .container_slots
            .iter()
            .map(|slot| slot.as_ref().map(encode_slot_data))
            .collect(),
    }
}
