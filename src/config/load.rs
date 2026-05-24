use super::model::EtleConfig;
use super::prelude::*;

pub fn load() -> Result<EtleConfig, ConfigError> {
    let Some(path) = default_config_path() else {
        return Ok(EtleConfig::default());
    };

    from_path(path)
}

pub fn from_path(path: impl AsRef<Path>) -> Result<EtleConfig, ConfigError> {
    let path = path.as_ref();
    match fs::read_to_string(path) {
        Ok(source) => super::parse::parse(&source),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(EtleConfig::default()),
        Err(error) => Err(ConfigError::Io(error)),
    }
}

#[must_use]
pub fn default_config_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os(ETLE_CONFIG_ENV).map(PathBuf::from) {
        return Some(path);
    }

    if let Some(xdg_config_home) = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
        return Some(
            xdg_config_home
                .join(DEFAULT_CONFIG_DIR_NAME)
                .join(DEFAULT_CONFIG_FILE_NAME),
        );
    }

    env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join(DEFAULT_CONFIG_DIR_NAME)
            .join(DEFAULT_CONFIG_FILE_NAME)
    })
}

#[must_use]
pub fn default_listen_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_P2P_PORT)
}
