pub(super) use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, File},
    io::{BufWriter, ErrorKind, Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, mpsc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(super) use tokio::net::{TcpListener, TcpStream};

pub(super) use crate::{
    crypto::{
        aead::{SymmetricKey, build_chunk_aad, decrypt_chunk, encrypt_chunk, generate_nonce},
        hash::{FileId, hash_chunk, hash_file},
        key_exchange::derive_session_key,
        key_wrap::{WrappedFileKey, generate_file_key, unwrap_file_key, wrap_file_key},
    },
    file::{
        descriptor::{EtleDescriptor, FileEntry, ShareId},
        error::FileError,
        manifest::{ChunkMeta, Manifest},
        storage::{EncryptedChunk, EncryptedFile, decrypt_to_file},
    },
    ipc::{message::IpcEvent, server::events},
    network::{
        error::NetworkError,
        handshake::{client_protocol_handshake, server_protocol_handshake},
        key_exchange::{client_shared_secret_exchange, server_shared_secret_exchange},
        tcp::{accept_peer, connect_peer},
    },
    protocol::{
        codec::{ReceivedChunkFrame, receive, receive_chunk_to_file, send, send_chunk_file},
        error::ProtocolError,
        message::WireMessage,
    },
    state::{
        library,
        model::{DownloadProgress, ShareMode, ShareState},
        paths::LibraryPaths,
        storage::{
            has_chunk, read_chunk, read_descriptor, read_progress, read_secret, write_chunk,
            write_progress, write_state,
        },
    },
};

pub(super) const PROGRESS_FLUSH_CHUNK_INTERVAL: usize = 32;
pub(super) const PROGRESS_FLUSH_TIME_INTERVAL: Duration = Duration::from_millis(750);
pub(super) const RECONSTRUCT_WRITER_BUFFER_SIZE: usize = 8 * 1024 * 1024;

pub(super) use super::options::{DownloadFileOptions, ServeFileOptions, TransferLogLevel};

pub(super) trait MutexExpectLock<T> {
    fn expect_lock(&self, message: &str) -> std::sync::MutexGuard<'_, T>;
}

impl<T> MutexExpectLock<T> for Mutex<T> {
    fn expect_lock(&self, message: &str) -> std::sync::MutexGuard<'_, T> {
        self.lock().expect(message)
    }
}
