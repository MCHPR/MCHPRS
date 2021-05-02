use serde::Serialize;

#[derive(Serialize)]
struct NBTMapEntry<T: Serialize> {
    name: String,
    id: i32,
    element: T,
}

/// This is a format used in the current network protocol,
/// most notably used in the JoinGame packet.
#[derive(Serialize)]
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
