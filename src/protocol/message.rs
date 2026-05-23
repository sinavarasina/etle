use serde::{Deserialize, Serialize};

pub const ETLE_WIRE_PROTOCOL_VERSION: u16 = 2;
pub const CAPABILITY_RAW_CHUNK_FRAME: &str = "raw-chunk-frame-v1";
pub const CAPABILITY_WINDOWED_REQUESTS: &str = "windowed-requests-v1";

use crate::{
    crypto::{aead::Nonce, key_exchange::PublicKeyBytes},
    file::{descriptor::ShareId, manifest::Manifest},
};

/// Messages exchanged by ETLE peers.
///
/// This is intentionally small for Sprint 2: it only covers handshake,
/// key exchange, manifest transfer, chunk availability, and chunk transfer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireMessage {
    Hello {
        peer_id: String,
    },
    Capabilities {
        protocol_version: u16,
        features: Vec<String>,
    },
    KeyExchange {
        public_key: PublicKeyBytes,
    },
    RequestManifest,
    RequestShare {
        share_id: ShareId,
    },
    Manifest {
        manifest: Manifest,
    },
    WrappedFileKey {
        nonce: Nonce,
        data: Vec<u8>,
    },
    Have {
        chunks: Vec<u32>,
    },
    RequestChunk {
        index: u32,
    },
    Chunk {
        index: u32,
        data: Vec<u8>,
    },
    Error {
        message: String,
    },
}
