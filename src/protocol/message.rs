use serde::{Deserialize, Serialize};

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
    Hello { peer_id: String },
    KeyExchange { public_key: PublicKeyBytes },
    RequestManifest,
    RequestShare { share_id: ShareId },
    Manifest { manifest: Manifest },
    WrappedFileKey { nonce: Nonce, data: Vec<u8> },
    Have { chunks: Vec<u32> },
    RequestChunk { index: u32 },
    Chunk { index: u32, data: Vec<u8> },
    Error { message: String },
}
