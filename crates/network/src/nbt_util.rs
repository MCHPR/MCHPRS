use serde::Serialize;

pub type NBTCompound = nbt::Map<String, nbt::Value>;

#[derive(Serialize, Clone)]
struct NBTMapEntry<T: Serialize> {
    name: String,
    id: i32,
    element: T,
}

/// This is a format used in the current network protocol,
/// most notably used in the `JoinGame` packet.
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
