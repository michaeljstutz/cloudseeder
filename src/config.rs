use rand::Rng;
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG_PATH: &str = "cloudseeder.toml";
pub const DEFAULT_ADDR: &str = "127.0.0.1:8080";
pub const DEFAULT_TEMPLATES_DIR: &str = "./templates";
const PREFIX_LEN: usize = 6;
const PREFIX_CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    addr: Option<String>,
    prefix: Option<String>,
    templates_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub addr: SocketAddr,
    pub prefix: String,
    pub prefix_source: PrefixSource,
    pub config_path: Option<PathBuf>,
    pub templates_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixSource {
    Config,
    Generated,
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Parse(toml::de::Error),
    Addr(std::net::AddrParseError, String),
    InvalidPrefix { prefix: String, invalid: Vec<char> },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "reading config: {e}"),
            LoadError::Parse(e) => write!(f, "parsing config: {e}"),
            LoadError::Addr(e, s) => write!(f, "invalid addr {s:?}: {e}"),
            LoadError::InvalidPrefix { prefix, invalid } => {
                let mut uniq: Vec<char> = invalid.clone();
                uniq.sort_unstable();
                uniq.dedup();
                let shown: Vec<String> = uniq.iter().map(|c| format!("{c:?}")).collect();
                write!(
                    f,
                    "invalid prefix {prefix:?}: only [a-z0-9] allowed (invalid: {})",
                    shown.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for LoadError {}

impl Settings {
    /// Load settings from the given config path (or the default), with env-var overrides applied.
    pub fn load(config_path: Option<&Path>) -> Result<Self, LoadError> {
        let path = config_path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));

        let (file, used_path) = if path.exists() {
            (parse_file(&path)?, Some(path))
        } else {
            (FileConfig::default(), None)
        };

        resolve(file, used_path, EnvOverrides::FromProcess)
    }

    /// Parse the given file exactly — no env merging. Intended for tests and tools that
    /// want deterministic, environment-independent settings.
    pub fn from_file(path: &Path) -> Result<Self, LoadError> {
        let file = parse_file(path)?;
        resolve(file, Some(path.to_path_buf()), EnvOverrides::Ignored)
    }
}

enum EnvOverrides {
    FromProcess,
    Ignored,
}

fn parse_file(path: &Path) -> Result<FileConfig, LoadError> {
    let text = std::fs::read_to_string(path).map_err(LoadError::Io)?;
    toml::from_str(&text).map_err(LoadError::Parse)
}

fn resolve(
    file: FileConfig,
    used_path: Option<PathBuf>,
    env: EnvOverrides,
) -> Result<Settings, LoadError> {
    let (env_addr, env_templates_dir) = match env {
        EnvOverrides::FromProcess => (
            std::env::var("CLOUDSEEDER_ADDR").ok(),
            std::env::var("CLOUDSEEDER_TEMPLATES_DIR")
                .ok()
                .map(PathBuf::from),
        ),
        EnvOverrides::Ignored => (None, None),
    };
    let addr_str = env_addr
        .or(file.addr)
        .unwrap_or_else(|| DEFAULT_ADDR.to_string());
    let addr: SocketAddr = addr_str
        .parse()
        .map_err(|e| LoadError::Addr(e, addr_str.clone()))?;

    let (prefix, prefix_source) = match file.prefix {
        Some(p) if !p.trim().is_empty() => {
            validate_prefix(&p)?;
            (p, PrefixSource::Config)
        }
        _ => (generate_prefix(), PrefixSource::Generated),
    };

    let templates_dir = env_templates_dir
        .or(file.templates_dir)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_TEMPLATES_DIR));

    Ok(Settings {
        addr,
        prefix,
        prefix_source,
        config_path: used_path,
        templates_dir,
    })
}

fn generate_prefix() -> String {
    let mut rng = rand::thread_rng();
    (0..PREFIX_LEN)
        .map(|_| PREFIX_CHARSET[rng.gen_range(0..PREFIX_CHARSET.len())] as char)
        .collect()
}

fn validate_prefix(prefix: &str) -> Result<(), LoadError> {
    let invalid: Vec<char> = prefix
        .chars()
        .filter(|c| !c.is_ascii_lowercase() && !c.is_ascii_digit())
        .collect();
    if invalid.is_empty() {
        Ok(())
    } else {
        Err(LoadError::InvalidPrefix {
            prefix: prefix.to_string(),
            invalid,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize any test that mutates process env. `cargo test` runs tests in parallel by
    // default, and `std::env::set_var` is `unsafe` because concurrent env access is unsound.
    // Hold this lock around any call to set_var/remove_var. Poison-tolerant: if a previous
    // holder panicked, recover the inner guard and proceed.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn generated_prefix_is_six_lowercase_alnum_chars() {
        let p = generate_prefix();
        assert_eq!(p.len(), PREFIX_LEN);
        assert!(p
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn from_file_uses_configured_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(&path, "addr = \"0.0.0.0:9000\"\nprefix = \"abc123\"\n").unwrap();
        let s = Settings::from_file(&path).unwrap();
        assert_eq!(s.prefix, "abc123");
        assert_eq!(s.prefix_source, PrefixSource::Config);
        assert_eq!(s.addr.to_string(), "0.0.0.0:9000");
    }

    #[test]
    fn from_file_defaults_templates_dir_when_unset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(&path, "prefix = \"abc123\"\n").unwrap();
        let s = Settings::from_file(&path).unwrap();
        assert_eq!(s.templates_dir, PathBuf::from(DEFAULT_TEMPLATES_DIR));
    }

    #[test]
    fn from_file_uses_configured_templates_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(
            &path,
            "prefix = \"abc123\"\ntemplates_dir = \"/srv/cloudseeder/templates\"\n",
        )
        .unwrap();
        let s = Settings::from_file(&path).unwrap();
        assert_eq!(s.templates_dir, PathBuf::from("/srv/cloudseeder/templates"));
    }

    #[test]
    fn from_file_generates_prefix_when_blank() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(&path, "prefix = \"\"\n").unwrap();
        let s = Settings::from_file(&path).unwrap();
        assert_eq!(s.prefix.len(), PREFIX_LEN);
        assert_eq!(s.prefix_source, PrefixSource::Generated);
    }

    #[test]
    fn from_file_rejects_invalid_prefix_chars() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(&path, "prefix = \"Ab-12!\"\n").unwrap();
        let err = Settings::from_file(&path).unwrap_err();
        let LoadError::InvalidPrefix { prefix, invalid } = &err else {
            panic!("expected InvalidPrefix, got {err:?}");
        };
        assert_eq!(prefix, "Ab-12!");
        assert_eq!(invalid, &vec!['A', '-', '!']);
        let msg = err.to_string();
        assert!(msg.contains("only [a-z0-9] allowed"), "msg: {msg}");
        assert!(msg.contains("\"Ab-12!\""), "msg: {msg}");
    }

    #[test]
    fn from_file_rejects_uppercase_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(&path, "prefix = \"ABCDEF\"\n").unwrap();
        let err = Settings::from_file(&path).unwrap_err();
        assert!(matches!(err, LoadError::InvalidPrefix { .. }));
    }

    #[test]
    fn from_file_returns_parse_error_on_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(&path, "this is = not valid toml [[\n").unwrap();
        let err = Settings::from_file(&path).unwrap_err();
        assert!(matches!(err, LoadError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn from_file_rejects_unknown_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(
            &path,
            "addr = \"127.0.0.1:8080\"\nrandom_typo_field = \"value\"\n",
        )
        .unwrap();
        let err = Settings::from_file(&path).unwrap_err();
        let LoadError::Parse(ref e) = err else {
            panic!("expected Parse, got {err:?}");
        };
        let msg = e.to_string();
        assert!(
            msg.contains("random_typo_field") || msg.contains("unknown field"),
            "expected unknown-field message, got: {msg}"
        );
    }

    #[test]
    fn from_file_treats_whitespace_prefix_as_blank() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(&path, "prefix = \"   \"\n").unwrap();
        let s = Settings::from_file(&path).unwrap();
        assert_eq!(s.prefix_source, PrefixSource::Generated);
        assert_eq!(s.prefix.len(), PREFIX_LEN);
    }

    #[test]
    fn from_file_returns_addr_error_on_invalid_addr() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(&path, "addr = \"not-a-socket-addr\"\n").unwrap();
        let err = Settings::from_file(&path).unwrap_err();
        let LoadError::Addr(_, raw) = &err else {
            panic!("expected Addr, got {err:?}");
        };
        assert_eq!(raw, "not-a-socket-addr");
    }

    #[test]
    fn load_returns_defaults_when_explicit_path_missing() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev_addr = std::env::var("CLOUDSEEDER_ADDR").ok();
        let prev_templates = std::env::var("CLOUDSEEDER_TEMPLATES_DIR").ok();
        // SAFETY: env mutation is serialized via ENV_LOCK above.
        unsafe {
            std::env::remove_var("CLOUDSEEDER_ADDR");
            std::env::remove_var("CLOUDSEEDER_TEMPLATES_DIR");
        }

        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.toml");
        let s = Settings::load(Some(&missing)).unwrap();

        unsafe {
            match prev_addr {
                Some(v) => std::env::set_var("CLOUDSEEDER_ADDR", v),
                None => std::env::remove_var("CLOUDSEEDER_ADDR"),
            }
            match prev_templates {
                Some(v) => std::env::set_var("CLOUDSEEDER_TEMPLATES_DIR", v),
                None => std::env::remove_var("CLOUDSEEDER_TEMPLATES_DIR"),
            }
        }

        assert_eq!(s.addr.to_string(), DEFAULT_ADDR);
        assert_eq!(s.prefix_source, PrefixSource::Generated);
        assert_eq!(s.templates_dir, PathBuf::from(DEFAULT_TEMPLATES_DIR));
        assert_eq!(s.config_path, None);
    }

    #[test]
    fn load_applies_env_addr_override() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("CLOUDSEEDER_ADDR").ok();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(&path, "addr = \"127.0.0.1:9001\"\nprefix = \"abc123\"\n").unwrap();

        // SAFETY: env mutation is serialized via ENV_LOCK above.
        unsafe { std::env::set_var("CLOUDSEEDER_ADDR", "127.0.0.1:65001") };
        let s = Settings::load(Some(&path)).unwrap();

        match prev {
            Some(v) => unsafe { std::env::set_var("CLOUDSEEDER_ADDR", v) },
            None => unsafe { std::env::remove_var("CLOUDSEEDER_ADDR") },
        }

        assert_eq!(s.addr.to_string(), "127.0.0.1:65001");
        assert_eq!(s.prefix, "abc123");
    }

    #[test]
    fn from_file_ignores_ambient_env_overrides() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(
            &path,
            "addr = \"127.0.0.1:7001\"\ntemplates_dir = \"/from/file\"\n",
        )
        .unwrap();

        let prev_addr = std::env::var("CLOUDSEEDER_ADDR").ok();
        let prev_templates = std::env::var("CLOUDSEEDER_TEMPLATES_DIR").ok();
        // SAFETY: env mutation is serialized via ENV_LOCK above; no other thread reads or
        // writes the environment while this guard is held.
        unsafe {
            std::env::set_var("CLOUDSEEDER_ADDR", "127.0.0.1:65000");
            std::env::set_var("CLOUDSEEDER_TEMPLATES_DIR", "/from/env");
        }
        let s = Settings::from_file(&path).unwrap();
        unsafe {
            match prev_addr {
                Some(v) => std::env::set_var("CLOUDSEEDER_ADDR", v),
                None => std::env::remove_var("CLOUDSEEDER_ADDR"),
            }
            match prev_templates {
                Some(v) => std::env::set_var("CLOUDSEEDER_TEMPLATES_DIR", v),
                None => std::env::remove_var("CLOUDSEEDER_TEMPLATES_DIR"),
            }
        }

        assert_eq!(s.addr.to_string(), "127.0.0.1:7001");
        assert_eq!(s.templates_dir, PathBuf::from("/from/file"));
    }

    #[test]
    fn load_applies_env_templates_dir_override() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("CLOUDSEEDER_TEMPLATES_DIR").ok();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudseeder.toml");
        std::fs::write(
            &path,
            "prefix = \"abc123\"\ntemplates_dir = \"/from/file\"\n",
        )
        .unwrap();

        // SAFETY: env mutation is serialized via ENV_LOCK above.
        unsafe { std::env::set_var("CLOUDSEEDER_TEMPLATES_DIR", "/from/env") };
        let s = Settings::load(Some(&path)).unwrap();

        match prev {
            Some(v) => unsafe { std::env::set_var("CLOUDSEEDER_TEMPLATES_DIR", v) },
            None => unsafe { std::env::remove_var("CLOUDSEEDER_TEMPLATES_DIR") },
        }

        assert_eq!(s.templates_dir, PathBuf::from("/from/env"));
    }
}
