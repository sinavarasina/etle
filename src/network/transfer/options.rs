use std::path::PathBuf;

use crate::{crypto::key_exchange::AuthPsk, file::descriptor::ShareId};

pub(super) const DEFAULT_REQUEST_WINDOW: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransferLogLevel {
    #[default]
    Quiet,
    Normal,
    Verbose,
}

impl TransferLogLevel {
    #[must_use]
    pub const fn is_normal(self) -> bool {
        matches!(self, Self::Normal | Self::Verbose)
    }

    #[must_use]
    pub const fn is_verbose(self) -> bool {
        matches!(self, Self::Verbose)
    }
}

#[derive(Clone, Debug)]
pub struct ServeFileOptions {
    pub seeder_id: String,
    pub log_level: TransferLogLevel,
    pub library_root: Option<PathBuf>,
    pub auth_psk: Option<AuthPsk>,
}

impl ServeFileOptions {
    #[must_use]
    pub fn new(seeder_id: impl Into<String>, log_level: TransferLogLevel) -> Self {
        Self {
            seeder_id: seeder_id.into(),
            log_level,
            library_root: None,
            auth_psk: None,
        }
    }

    #[must_use]
    pub fn with_library_root(mut self, library_root: impl Into<PathBuf>) -> Self {
        self.library_root = Some(library_root.into());
        self
    }

    #[must_use]
    pub fn with_auth_psk(mut self, auth_psk: AuthPsk) -> Self {
        self.auth_psk = Some(auth_psk);
        self
    }
}

#[derive(Clone, Debug)]
pub struct DownloadFileOptions {
    pub peer_id: String,
    pub log_level: TransferLogLevel,
    pub library_root: Option<PathBuf>,
    pub resume: bool,
    pub requested_share_id: Option<ShareId>,
    pub request_window: usize,
    pub auth_psk: Option<AuthPsk>,
}

impl DownloadFileOptions {
    #[must_use]
    pub fn new(peer_id: impl Into<String>, log_level: TransferLogLevel) -> Self {
        Self {
            peer_id: peer_id.into(),
            log_level,
            library_root: None,
            resume: true,
            requested_share_id: None,
            request_window: DEFAULT_REQUEST_WINDOW,
            auth_psk: None,
        }
    }

    #[must_use]
    pub fn with_library_root(mut self, library_root: impl Into<PathBuf>) -> Self {
        self.library_root = Some(library_root.into());
        self
    }

    #[must_use]
    pub const fn with_resume(mut self, resume: bool) -> Self {
        self.resume = resume;
        self
    }

    #[must_use]
    pub const fn with_requested_share_id(mut self, share_id: Option<ShareId>) -> Self {
        self.requested_share_id = share_id;
        self
    }

    #[must_use]
    pub const fn with_request_window(mut self, request_window: usize) -> Self {
        self.request_window = request_window;
        self
    }

    #[must_use]
    pub fn with_auth_psk(mut self, auth_psk: AuthPsk) -> Self {
        self.auth_psk = Some(auth_psk);
        self
    }
}
