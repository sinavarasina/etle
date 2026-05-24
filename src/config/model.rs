use super::prelude::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EtleConfig {
    pub library_root: Option<PathBuf>,
    pub ipc_socket: Option<PathBuf>,
    pub listen: Option<SocketAddr>,
    pub discovery_port: Option<u16>,
    pub discovery_multicast: Option<Ipv4Addr>,
    pub discovery_timeout_ms: Option<u64>,
    pub request_window: Option<usize>,
    pub parallel: Option<usize>,
    pub auth_psk: Option<String>,
}

impl EtleConfig {
    #[must_use]
    pub fn library_root(&self) -> Option<PathBuf> {
        self.library_root
            .as_deref()
            .map(super::parse::expand_tilde_path)
    }

    #[must_use]
    pub fn ipc_socket(&self) -> Option<PathBuf> {
        self.ipc_socket
            .as_deref()
            .map(super::parse::expand_tilde_path)
    }

    #[must_use]
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen.unwrap_or_else(super::load::default_listen_addr)
    }

    #[must_use]
    pub fn discovery_port(&self) -> u16 {
        self.discovery_port.unwrap_or(DEFAULT_DISCOVERY_PORT)
    }

    #[must_use]
    pub fn discovery_multicast(&self) -> Ipv4Addr {
        self.discovery_multicast
            .unwrap_or(DEFAULT_DISCOVERY_MULTICAST_ADDR)
    }

    #[must_use]
    pub fn discovery_timeout_ms(&self) -> u64 {
        self.discovery_timeout_ms
            .unwrap_or(DEFAULT_DISCOVERY_TIMEOUT_MS)
    }

    #[must_use]
    pub fn request_window(&self) -> usize {
        self.request_window.unwrap_or(DEFAULT_REQUEST_WINDOW)
    }

    #[must_use]
    pub fn parallel(&self) -> usize {
        self.parallel.unwrap_or(DEFAULT_DOWNLOAD_PARALLELISM)
    }

    #[must_use]
    pub fn auth_psk(&self) -> Option<String> {
        self.auth_psk.clone()
    }
}
