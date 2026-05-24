use std::net::Ipv4Addr;

pub const DEFAULT_DISCOVERY_PORT: u16 = 37037;
pub const DEFAULT_DISCOVERY_TIMEOUT_MS: u64 = 3000;
pub const DEFAULT_DISCOVERY_MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 0, 86);

pub(super) const DISCOVERY_MAGIC: &str = "etle-discovery-v1";
pub(super) const MAX_DISCOVERY_PACKET_SIZE: usize = 4096;
