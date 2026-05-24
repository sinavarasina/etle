pub(super) use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use crate::{
    crypto::aead::SymmetricKey,
    file::{
        descriptor::{EtleDescriptor, ShareId},
        error::FileError,
        storage::EncryptedChunk,
    },
};

pub const ETLE_DIR_NAME: &str = ".etle";
