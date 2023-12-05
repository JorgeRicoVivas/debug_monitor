#[cfg(feature = "use_nanoserde")]
use nanoserde::{DeJson, SerJson};
#[cfg(feature = "use_serde")]
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub trait JSONDeSerializable: Sized {
    fn to_json(&self) -> Option<String>;
    fn from_json(json: &str) -> Option<Self>;
}

#[cfg(feature = "use_serde")]
impl<T> JSONDeSerializable for T where T: Serialize + DeserializeOwned {
    fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }

    fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}

#[cfg(feature = "use_nanoserde")]
impl<T> JSONDeSerializable for T where T: nanoserde::SerJson + nanoserde::DeJson {
    fn to_json(&self) -> Option<String> {
        Some(self.serialize_json())
    }

    fn from_json(json: &str) -> Option<Self> {
        Self::deserialize_json(json).ok()
    }
}

#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "use_nanoserde", derive(SerJson, DeJson))]
pub enum ServerMessage {
    GiveClientId {
        client_id: usize
    },
    Notify {
        id: usize,
        name: String,
        value_in_json: String,
    },
    Remove {
        id: usize
    },
    RemoveAll,
}

#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "use_nanoserde", derive(SerJson, DeJson))]
pub enum ClientUnitMessage {
    UpdateValue {
        id: usize,
        new_value: String,
    },
    Renotify,
}
