//! Remote session DTOs (served by life-relayd) — v0.2 reserved.
//!
//! Trait is shipped in Phase 0 so `life-kernel-proto` can reference it, but
//! facade implementations return `KernelError::Unimplemented` until v0.2
//! lights the proxies up.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RelayToken(pub String);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RelayOpenRequest {
    pub kind: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelaySession {
    pub token: RelayToken,
    pub opened_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayFrame {
    pub seq: u64,
    #[serde(with = "bytes_as_base64")]
    pub payload: Bytes,
}

mod bytes_as_base64 {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use bytes::Bytes;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(b: &Bytes, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(b))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Bytes, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD
            .decode(s)
            .map(Bytes::from)
            .map_err(serde::de::Error::custom)
    }
}
