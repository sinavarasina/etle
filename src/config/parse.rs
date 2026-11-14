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
            "library_root" => {
                config.library_root =
                    Some(PathBuf::from(parse_string_value(line_number, key, value)?))
            }
            "ipc_socket" => {
                config.ipc_socket =
                    Some(PathBuf::from(parse_string_value(line_number, key, value)?))
            }
            "listen" => {
                let raw = parse_string_value(line_number, key, value)?;
                config.listen = Some(parse_value(line_number, key, &raw)?);
            }
            "discovery_port" => {
                let port: u16 = parse_value(line_number, key, value)?;
                if port == 0 {
                    return Err(ConfigError::InvalidValue {
                        line: line_number,
                        key: key.to_string(),
                        value: value.to_string(),
                    });
                }
                config.discovery_port = Some(port);
            }
            "discovery_multicast" => {
                let raw = parse_string_value(line_number, key, value)?;
                let multicast: Ipv4Addr = parse_value(line_number, key, &raw)?;
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
                let timeout_ms: u64 = parse_value(line_number, key, value)?;
                if timeout_ms == 0 {
                    return Err(ConfigError::InvalidValue {
                        line: line_number,
                        key: key.to_string(),
                        value: value.to_string(),
                    });
                }
                config.discovery_timeout_ms = Some(timeout_ms);
            }
            "request_window" => {
                let window: usize = parse_value(line_number, key, value)?;
                if window == 0 {
                    return Err(ConfigError::InvalidValue {
                        line: line_number,
                        key: key.to_string(),
                        value: value.to_string(),
                    });
                }
                config.request_window = Some(window);
            }
            "parallel" => config.parallel = Some(parse_value(line_number, key, value)?),
            "auth_psk" => config.auth_psk = Some(parse_string_value(line_number, key, value)?),
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

fn parse_string_value(line: usize, key: &str, value: &str) -> Result<String, ConfigError> {
    let value = value.trim();
    let parsed = if let Some(after_open) = value.strip_prefix('"') {
        parse_quoted_string(line, key, after_open)?
    } else {
        if value.contains('"') || value.chars().any(char::is_whitespace) {
            return Err(ConfigError::InvalidValue {
                line,
                key: key.to_string(),
                value: value.to_string(),
            });
        }
        value.to_string()
    };

    if parsed.trim().is_empty() {
        return Err(ConfigError::InvalidValue {
            line,
            key: key.to_string(),
            value: value.to_string(),
        });
    }

    Ok(parsed)
}

fn parse_quoted_string(line: usize, key: &str, after_open: &str) -> Result<String, ConfigError> {
    let mut output = String::with_capacity(after_open.len());
    let mut chars = after_open.chars();
    let mut closed = false;

    while let Some(ch) = chars.next() {
        if ch == '"' {
            if !chars.as_str().trim().is_empty() {
                return Err(ConfigError::InvalidValue {
                    line,
                    key: key.to_string(),
                    value: format!("\"{after_open}"),
                });
            }
            closed = true;
            break;
        }

        if ch != '\\' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('"') => output.push('"'),
            Some('\\') => output.push('\\'),
            Some('n') => output.push('\n'),
            Some('t') => output.push('\t'),
            Some('r') => output.push('\r'),
            _ => {
                return Err(ConfigError::InvalidValue {
                    line,
                    key: key.to_string(),
                    value: format!("\"{after_open}"),
                });
            }
        }
    }

    if !closed {
        return Err(ConfigError::InvalidValue {
            line,
            key: key.to_string(),
            value: format!("\"{after_open}"),
        });
    }

    Ok(output)
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

    if let Some(rest) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
        && let Some(home) = home_dir()
    {
        return home.join(rest);
    }

    path.to_path_buf()
}

fn home_dir() -> Option<PathBuf> {
    crate::state::paths::home_dir_from_env()
}
