use std::{
    env, fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::discovery::{
    DEFAULT_DISCOVERY_MULTICAST_ADDR, DEFAULT_DISCOVERY_PORT, DEFAULT_DISCOVERY_TIMEOUT_MS,
};

pub const ETLE_CONFIG_ENV: &str = "ETLE_CONFIG";
pub const DEFAULT_CONFIG_DIR_NAME: &str = "etle";
pub const DEFAULT_CONFIG_FILE_NAME: &str = "config.toml";
pub const DEFAULT_DOWNLOAD_PARALLELISM: usize = 0;
pub const DEFAULT_REQUEST_WINDOW: usize = 16;
pub const DEFAULT_P2P_PORT: u16 = 7000;

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
}

impl EtleConfig {
    #[must_use]
    pub fn library_root(&self) -> Option<PathBuf> {
        self.library_root.as_deref().map(expand_tilde_path)
    }

    #[must_use]
    pub fn ipc_socket(&self) -> Option<PathBuf> {
        self.ipc_socket.as_deref().map(expand_tilde_path)
    }

    #[must_use]
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen.unwrap_or_else(default_listen_addr)
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
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid config line {line}: {message}")]
    InvalidLine { line: usize, message: String },

    #[error("invalid value for config key `{key}` on line {line}: {value}")]
    InvalidValue {
        line: usize,
        key: String,
        value: String,
    },

    #[error("unknown config key `{key}` on line {line}")]
    UnknownKey { line: usize, key: String },
}

pub fn load_config() -> Result<EtleConfig, ConfigError> {
    let Some(path) = default_config_path() else {
        return Ok(EtleConfig::default());
    };

    load_config_from_path(path)
}

pub fn load_config_from_path(path: impl AsRef<Path>) -> Result<EtleConfig, ConfigError> {
    let path = path.as_ref();
    match fs::read_to_string(path) {
        Ok(source) => parse_config(&source),
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

fn parse_config(source: &str) -> Result<EtleConfig, ConfigError> {
    let mut config = EtleConfig::default();

    for (line_index, raw_line) in source.lines().enumerate() {
        let line_number = line_index + 1;
        let stripped_line = strip_comment(raw_line);
        let line = stripped_line.trim();
        if line.is_empty() {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(ConfigError::InvalidLine {
                line: line_number,
                message: "expected `key = value`".to_string(),
            });
        };

        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return Err(ConfigError::InvalidLine {
                line: line_number,
                message: "key and value must be non-empty".to_string(),
            });
        }

        match key {
            "library_root" => config.library_root = Some(PathBuf::from(parse_string_value(value))),
            "ipc_socket" => config.ipc_socket = Some(PathBuf::from(parse_string_value(value))),
            "listen" => {
                config.listen = Some(parse_value(line_number, key, &parse_string_value(value))?)
            }
            "discovery_port" => config.discovery_port = Some(parse_value(line_number, key, value)?),
            "discovery_multicast" => {
                let multicast: Ipv4Addr =
                    parse_value(line_number, key, &parse_string_value(value))?;
                if !multicast.is_multicast() {
                    return Err(ConfigError::InvalidValue {
                        line: line_number,
                        key: key.to_string(),
                        value: multicast.to_string(),
                    });
                }
                config.discovery_multicast = Some(multicast);
            }
            "discovery_timeout_ms" => {
                config.discovery_timeout_ms = Some(parse_value(line_number, key, value)?)
            }
            "request_window" => config.request_window = Some(parse_value(line_number, key, value)?),
            "parallel" => config.parallel = Some(parse_value(line_number, key, value)?),
            _ => {
                return Err(ConfigError::UnknownKey {
                    line: line_number,
                    key: key.to_string(),
                });
            }
        }
    }

    Ok(config)
}

fn parse_value<T>(line: usize, key: &str, value: &str) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
{
    value.parse().map_err(|_| ConfigError::InvalidValue {
        line,
        key: key.to_string(),
        value: value.to_string(),
    })
}

fn parse_string_value(value: &str) -> String {
    let value = value.trim();
    if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        return unescape_basic_string(inner);
    }

    value.to_string()
}

fn unescape_basic_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('"') => output.push('"'),
            Some('\\') => output.push('\\'),
            Some('n') => output.push('\n'),
            Some('t') => output.push('\t'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }

    output
}

fn strip_comment(line: &str) -> String {
    let mut output = String::new();
    let mut in_string = false;
    let mut escaped = false;

    for ch in line.chars() {
        if escaped {
            output.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' && in_string {
            output.push(ch);
            escaped = true;
            continue;
        }

        if ch == '"' {
            in_string = !in_string;
            output.push(ch);
            continue;
        }

        if ch == '#' && !in_string {
            break;
        }

        output.push(ch);
    }

    output
}

fn expand_tilde_path(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    if value == "~" {
        return home_dir().unwrap_or_else(|| path.to_path_buf());
    }

    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = home_dir()
    {
        return home.join(rest);
    }

    path.to_path_buf()
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_config_file() {
        let config = parse_config(
            r#"
            library_root = "~/Downloads/ETLE"
            ipc_socket = "/tmp/etled.sock"
            listen = "0.0.0.0:7000"
            discovery_port = 37037
            discovery_multicast = "239.255.0.86"
            discovery_timeout_ms = 5000
            request_window = 32
            parallel = 0
            "#,
        )
        .unwrap();

        assert_eq!(config.library_root, Some(PathBuf::from("~/Downloads/ETLE")));
        assert_eq!(config.ipc_socket, Some(PathBuf::from("/tmp/etled.sock")));
        assert_eq!(config.listen, Some(default_listen_addr()));
        assert_eq!(config.discovery_port, Some(37037));
        assert_eq!(
            config.discovery_multicast,
            Some(DEFAULT_DISCOVERY_MULTICAST_ADDR)
        );
        assert_eq!(config.discovery_timeout_ms, Some(5000));
        assert_eq!(config.request_window, Some(32));
        assert_eq!(config.parallel, Some(0));
    }

    #[test]
    fn ignores_comments_outside_strings() {
        let config = parse_config(
            r#"
            library_root = "/tmp/#not-comment" # comment
            request_window = 16 # comment
            "#,
        )
        .unwrap();

        assert_eq!(
            config.library_root,
            Some(PathBuf::from("/tmp/#not-comment"))
        );
        assert_eq!(config.request_window, Some(16));
    }

    #[test]
    fn rejects_non_multicast_discovery_multicast() {
        assert!(parse_config(r#"discovery_multicast = "192.168.1.1""#).is_err());
    }
}
