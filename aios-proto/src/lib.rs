//! Generated aios.v1 proto types.
//!
//! All types live under `aios::v1` (the proto package path). Internal
//! substrates should depend on `aios-protocol` (Layer-1 hand-written types)
//! and use the `proto_bridge` module there to convert at the wire boundary.

#![deny(unsafe_code)]
#![allow(missing_docs)] // generated code

#[allow(unused_qualifications, clippy::all)]
pub mod aios {
    pub mod v1 {
        tonic::include_proto!("aios.v1");
    }
}
