use std::fmt;
use std::str::FromStr;

use indexmap::IndexMap;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::collections::GetKey;

#[allow(dead_code)]
pub fn bool_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrStruct;

    impl<'de> Visitor<'de> for StringOrStruct {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or boolean")
        }

        fn visit_str<E>(self, value: &str) -> Result<String, E>
        where
            E: de::Error,
        {
            Ok(FromStr::from_str(value).unwrap())
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            match v {
                true => Err(de::Error::custom("Unexpected true in bool_string")),
                false => Ok(FromStr::from_str("").unwrap()),
            }
        }
    }

    deserializer.deserialize_any(StringOrStruct)
}

pub fn serialize_map_values<S: Serializer, K, V: Clone + Serialize>(
    map: &IndexMap<K, V>,
    s: S,
) -> Result<S::Ok, S::Error> {
    map.iter()
        .map(|(_, v)| v.clone())
        .collect::<Vec<_>>()
        .serialize(s)
}

pub fn deserialize_map_values<'de, D, T: ?Sized + GetKey>(
    d: D,
) -> Result<IndexMap<String, T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let data = <Vec<T>>::deserialize(d)?;

    let mapped = data
        .into_iter()
        .map(|elem| (elem.get_key().to_string(), elem))
        .collect();

    Ok(mapped)
}
