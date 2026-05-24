use super::model::EtleConfig;
use super::prelude::*;

pub(super) fn parse(source: &str) -> Result<EtleConfig, ConfigError> {
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
            "auth_psk" => config.auth_psk = Some(parse_string_value(value)),
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

pub(super) fn expand_tilde_path(path: &Path) -> PathBuf {
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
