use super::model::EtleConfig;
use super::prelude::*;

enum ConfigPathSource {
    Explicit(PathBuf),
    Implicit(PathBuf),
}

pub fn load() -> Result<EtleConfig, ConfigError> {
    match default_config_source() {
        None => Ok(EtleConfig::default()),
        Some(ConfigPathSource::Explicit(path)) => {
            let source = fs::read_to_string(path)?;
            super::parse::parse(&source)
        }
        Some(ConfigPathSource::Implicit(path)) => from_path(path),
    }
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
    default_config_source().map(|source| match source {
        ConfigPathSource::Explicit(path) | ConfigPathSource::Implicit(path) => path,
    })
}

fn default_config_source() -> Option<ConfigPathSource> {
    if let Some(path) = env::var_os(ETLE_CONFIG_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Some(ConfigPathSource::Explicit(path));
    }

    if let Some(xdg_config_home) = env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Some(ConfigPathSource::Implicit(
            xdg_config_home
                .join(DEFAULT_CONFIG_DIR_NAME)
                .join(DEFAULT_CONFIG_FILE_NAME),
        ));
    }

    if cfg!(windows)
        && let Some(appdata) = env::var_os("APPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    {
        return Some(ConfigPathSource::Implicit(
            appdata
                .join(DEFAULT_CONFIG_DIR_NAME)
                .join(DEFAULT_CONFIG_FILE_NAME),
        ));
    }

    crate::state::paths::home_dir_from_env().map(|home| {
        ConfigPathSource::Implicit(
            home.join(".config")
                .join(DEFAULT_CONFIG_DIR_NAME)
                .join(DEFAULT_CONFIG_FILE_NAME),
        )
    })
}

#[must_use]
pub fn default_listen_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_P2P_PORT)
}
