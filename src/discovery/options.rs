use super::prelude::*;

pub use super::constants::{
    DEFAULT_DISCOVERY_MULTICAST_ADDR, DEFAULT_DISCOVERY_PORT, DEFAULT_DISCOVERY_TIMEOUT_MS,
};

pub struct DiscoveryOptions {
    pub port: u16,
    pub timeout: Duration,
    pub multicast: Option<Ipv4Addr>,
    pub verbose: bool,
}

impl DiscoveryOptions {
    #[must_use]
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            timeout: Duration::from_millis(DEFAULT_DISCOVERY_TIMEOUT_MS),
            multicast: Some(DEFAULT_DISCOVERY_MULTICAST_ADDR),
            verbose: false,
        }
    }

    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub fn with_multicast(mut self, multicast: Ipv4Addr) -> Self {
        self.multicast = multicast.is_multicast().then_some(multicast);
        self
    }

    #[must_use]
    pub const fn without_multicast(mut self) -> Self {
        self.multicast = None;
        self
    }

    #[must_use]
    pub const fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
}
