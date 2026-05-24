#![allow(clippy::collapsible_if, clippy::while_let_loop)]
//! High-level file transfer orchestration.
//!
//! Public API is grouped by operation:
//! - `transfer::seed::add`
//! - `transfer::serve::{file_once, library_once, share_once, library_forever}`
//! - `transfer::download::{from_peer, from_peers, from_peers_parallel}`
//! - `transfer::jobs::{register, unregister}`

mod prelude;

pub mod download;
pub mod jobs;
pub mod options;
pub mod progress;
pub mod seed;
pub mod serve;
