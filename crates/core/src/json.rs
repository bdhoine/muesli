//! JSON conventions shared with the Swift app: ISO 8601 dates without fractional seconds,
//! sorted keys, optional fields omitted when absent.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Serde adapter for `DateTime<Utc>`: writes `2026-11-10T14:30:00Z`, reads any RFC 3339 date.
pub mod iso_date {
    use super::*;

    pub fn serialize<S: Serializer>(d: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&d.to_rfc3339_opts(SecondsFormat::Secs, true))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<DateTime<Utc>, D::Error> {
        let text = String::deserialize(d)?;
        DateTime::parse_from_rfc3339(&text)
            .map(|d| d.with_timezone(&Utc))
            .map_err(serde::de::Error::custom)
    }
}

/// Same, for `Option<DateTime<Utc>>`. Pair with `skip_serializing_if = "Option::is_none"`.
pub mod iso_date_opt {
    use super::*;

    pub fn serialize<S: Serializer>(d: &Option<DateTime<Utc>>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(d) => iso_date::serialize(d, s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<DateTime<Utc>>, D::Error> {
        let text = Option::<String>::deserialize(d)?;
        text.map(|t| {
            DateTime::parse_from_rfc3339(&t)
                .map(|d| d.with_timezone(&Utc))
                .map_err(serde::de::Error::custom)
        })
        .transpose()
    }
}

/// Pretty-printed JSON with keys sorted alphabetically, like Swift's `.prettyPrinted, .sortedKeys`.
pub fn to_pretty_sorted<T: Serialize>(value: &T) -> String {
    // `serde_json::Value` objects are ordered maps by default, which sorts the keys.
    let value = serde_json::to_value(value).expect("serializable");
    serde_json::to_string_pretty(&value).expect("printable")
}

pub fn from_str<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_str(text).ok()
}

/// Writes through a uniquely named temporary file in the same folder and renames it into place,
/// so readers never see half a file and two writers (the app and muesli-mcp) never share a temp file.
pub fn write_atomic(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(contents.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map(|_| ()).map_err(|e| e.error)
}
