//! A map with number keys on the wire, for
//! `#[serde(with = "riff_core::number_keys")]` (01M74BVSFDAE67FA1PRYK7E2PN).
//!
//! A key of a JSON object is text, so serde_json writes the key 724 as
//! `"724"`. serde_json reads that key back as a number in a plain map.
//! serde reads a map in an internally tagged enum, in an untagged enum
//! or in a `#[serde(flatten)]` through buffered content. There the key
//! stays text, and the read of a number key fails: `invalid type: string
//! "724", expected u64`. The status line of the riff daemon is such a
//! map: the status is in the tagged enum of the lines of the daemon, and
//! the waits of a wave are by issue (#1855).
//!
//! [`deserialize`] reads each key from text or from a number, so the map
//! reads in each place. [`serialize`] writes the map as serde writes it:
//! the keys are text on the wire, and numbers in the code.
//!
//! ```mermaid
//! flowchart LR
//!     M["BTreeMap: 724 → 728"] -- serialize --> J["{&quot;724&quot;: 728}"]
//!     J -- "a plain map" --> K["the key: text or a number"]
//!     J -- "a tagged enum, an untagged enum, a flatten" --> C["buffered content: the key is text"] --> K
//!     K -- deserialize --> M
//! ```
//!
//! ```
//! use std::collections::BTreeMap;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, PartialEq, Serialize, Deserialize)]
//! #[serde(tag = "say", rename_all = "snake_case")]
//! enum Say {
//!     Wave {
//!         #[serde(with = "riff_core::number_keys")]
//!         waits: BTreeMap<u64, u64>,
//!     },
//! }
//!
//! let say = Say::Wave { waits: BTreeMap::from([(724, 728)]) };
//! let line = serde_json::to_string(&say)?;
//! assert_eq!(line, r#"{"say":"wave","waits":{"724":728}}"#);
//! assert_eq!(serde_json::from_str::<Say>(&line)?, say);
//! # Ok::<(), serde_json::Error>(())
//! ```

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserializer, Unexpected, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// Writes `map` as serde writes a map: serde_json writes each key as
/// text.
///
/// ```
/// use std::collections::BTreeMap;
///
/// let mut out = Vec::new();
/// let map = BTreeMap::from([(724u64, 728u64), (725, 728)]);
/// riff_core::number_keys::serialize(&map, &mut serde_json::Serializer::new(&mut out))?;
/// assert_eq!(String::from_utf8(out).unwrap(), r#"{"724":728,"725":728}"#);
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn serialize<V: Serialize, S: Serializer>(
    map: &BTreeMap<u64, V>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    map.serialize(serializer)
}

/// Reads a map whose keys are numbers, each key from text or from a
/// number. A key that is no number fails.
///
/// ```
/// use std::collections::BTreeMap;
///
/// let read = |json: &str| {
///     riff_core::number_keys::deserialize::<u64, _>(&mut serde_json::Deserializer::from_str(json))
/// };
/// assert_eq!(read(r#"{"724":728}"#)?, BTreeMap::from([(724, 728)]));
/// assert!(read(r#"{}"#)?.is_empty());
/// let error = read(r#"{"issue-724":728}"#).unwrap_err().to_string();
/// assert!(error.contains("a number, or a number in text"), "{error}");
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn deserialize<'de, V: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<u64, V>, D::Error> {
    let map = BTreeMap::<Key, V>::deserialize(deserializer)?;
    Ok(map
        .into_iter()
        .map(|(Key(key), value)| (key, value))
        .collect())
}

/// One key of the map: a number, read from text or from a number.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Key(u64);

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Key, D::Error> {
        deserializer.deserialize_any(KeyVisitor)
    }
}

struct KeyVisitor;

impl Visitor<'_> for KeyVisitor {
    type Value = Key;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a number, or a number in text")
    }

    fn visit_u64<E: de::Error>(self, key: u64) -> Result<Key, E> {
        Ok(Key(key))
    }

    fn visit_str<E: de::Error>(self, key: &str) -> Result<Key, E> {
        key.parse()
            .map(Key)
            .map_err(|_| E::invalid_value(Unexpected::Str(key), &self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map of number keys with the helper, and one with no helper.
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Waits {
        #[serde(default, with = "super")]
        waits: BTreeMap<u64, u64>,
    }

    #[derive(Debug, PartialEq, Deserialize)]
    struct Plain {
        waits: BTreeMap<u64, u64>,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "say", rename_all = "snake_case")]
    enum Tagged {
        Status(Waits),
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    enum Untagged {
        Status(Waits),
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Flat {
        repo: String,
        #[serde(flatten)]
        waits: Waits,
    }

    fn waits() -> Waits {
        Waits {
            waits: BTreeMap::from([(724, 728), (725, 728), (730, 731)]),
        }
    }

    /// #1855: with no helper, a map of number keys reads in a plain map
    /// and fails in a tagged enum. The helper reads it in both.
    #[test]
    fn a_number_key_reads_in_a_tagged_enum_only_with_the_helper() {
        #[derive(Debug, Deserialize)]
        #[serde(tag = "say", rename_all = "snake_case")]
        enum NoHelper {
            Status(#[allow(dead_code)] Plain),
        }
        let line = r#"{"say":"status","waits":{"724":728}}"#;
        let plain: Plain = serde_json::from_str(r#"{"waits":{"724":728}}"#).unwrap();
        assert_eq!(plain.waits, BTreeMap::from([(724, 728)]));
        let error = serde_json::from_str::<NoHelper>(line).unwrap_err();
        assert!(
            error
                .to_string()
                .contains(r#"invalid type: string "724", expected u64"#),
            "{error}"
        );
        let Tagged::Status(read) = serde_json::from_str(line).unwrap();
        assert_eq!(read.waits, BTreeMap::from([(724, 728)]));
    }

    /// Each place of a map reads the line that it writes: a plain
    /// struct, a tagged enum, an untagged enum and a flatten, from text
    /// and from a `serde_json::Value`.
    #[test]
    fn the_map_reads_again_in_each_place() {
        fn again<T: Serialize + for<'de> Deserialize<'de> + PartialEq + fmt::Debug>(value: &T) {
            let line = serde_json::to_string(value).unwrap();
            assert!(line.contains(r#""724":728"#), "{line}");
            assert_eq!(&serde_json::from_str::<T>(&line).unwrap(), value);
            let json = serde_json::to_value(value).unwrap();
            assert_eq!(&serde_json::from_value::<T>(json).unwrap(), value);
        }
        again(&waits());
        again(&Tagged::Status(waits()));
        again(&Untagged::Status(waits()));
        again(&Flat {
            repo: "o/r".into(),
            waits: waits(),
        });
    }

    /// A key that a number writes, for example in another format than
    /// JSON, reads too. A key that is no number, or a negative one,
    /// fails.
    #[test]
    fn a_key_reads_from_a_number_and_fails_for_no_number() {
        use serde::de::value::{Error, MapDeserializer};

        let numbers = MapDeserializer::<_, Error>::new([(724u64, 728u64), (725, 728)].into_iter());
        assert_eq!(
            deserialize::<u64, _>(numbers).unwrap(),
            BTreeMap::from([(724, 728), (725, 728)])
        );
        for key in ["issue-724", "-724", ""] {
            let line = format!(r#"{{"say":"status","waits":{{"{key}":728}}}}"#);
            let error = serde_json::from_str::<Tagged>(&line).unwrap_err();
            assert!(
                error.to_string().contains("a number, or a number in text"),
                "{key}: {error}"
            );
        }
    }
}
