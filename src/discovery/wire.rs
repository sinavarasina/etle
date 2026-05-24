use std::net::{Ipv4Addr, SocketAddr};

use serde::{Deserialize, Serialize};

use crate::file::descriptor::ShareId;

pub(super) struct DiscoveryInterface {
    pub(super) ip: Ipv4Addr,
    pub(super) broadcast: Option<Ipv4Addr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum DiscoveryMessage {
    Query {
        magic: String,
        share_id: ShareId,
    },
    Response {
        magic: String,
        share_id: ShareId,
        listen_addr: SocketAddr,
        listen_port: u16,
        peer_id: String,
        instance_id: String,
        name: String,
    },
}
