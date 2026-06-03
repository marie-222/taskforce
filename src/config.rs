// Copyright (c) 2026- Masaki Ishii
// Copyright (c) 2026- Small Gear Lab
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

#[derive(Clone, Default, Deserialize)]
pub struct AppConfig {
    /// Legacy SQLite path. Prefer `[backend].sqlite_path` for new configs.
    pub sqlite_path: Option<PathBuf>,
    #[serde(default)]
    pub backend: BackendConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub list: ListConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListConfig {
    #[serde(default = "default_open_statuses")]
    pub open_statuses: Vec<String>,
}

impl Default for ListConfig {
    fn default() -> Self {
        Self {
            open_statuses: default_open_statuses(),
        }
    }
}

fn default_open_statuses() -> Vec<String> {
    vec!["active".to_string()]
}

#[derive(Clone, Default, Deserialize)]
struct FileListConfig {
    pub open_statuses: Option<Vec<String>>,
}

#[derive(Clone, Default, Deserialize)]
struct FileAppConfig {
    pub sqlite_path: Option<PathBuf>,
    pub profiles: Option<ProfilesConfig>,
    pub backend: Option<FileBackendConfig>,
    pub server: Option<FileServerConfig>,
    pub list: Option<FileListConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProfilesConfig {
    pub default: Option<String>,
    #[serde(default)]
    pub items: Vec<ProfileItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileItem {
    pub id: String,
    pub config: Option<PathBuf>,
    pub env_file: Option<PathBuf>,
}

#[derive(Clone, Default, Deserialize)]
struct FileBackendConfig {
    #[serde(alias = "type")]
    pub kind: Option<BackendKind>,
    pub sqlite_path: Option<PathBuf>,
    pub postgres_url: Option<String>,
    pub postgres_ssl_root_cert: Option<PathBuf>,
}

#[derive(Clone, Default, Deserialize)]
struct FileServerConfig {
    pub host: Option<IpAddr>,
    pub port: Option<u16>,
}

#[derive(Clone, Default, Deserialize)]
pub struct BackendConfig {
    #[serde(default, alias = "type")]
    pub kind: BackendKind,
    pub sqlite_path: Option<PathBuf>,
    pub postgres_url: Option<String>,
    pub postgres_ssl_root_cert: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    #[default]
    Sqlite,
    Postgres,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServerConfig {
    pub host: Option<IpAddr>,
    pub port: Option<u16>,
}

impl ServerConfig {
    pub fn resolve(&self) -> Result<SocketAddr> {
        let host = server_host_env()
            .transpose()?
            .or(self.host)
            .unwrap_or(IpAddr::from([127, 0, 0, 1]));
        let port = server_port_env()?.or(self.port).unwrap_or(0);
        Ok(SocketAddr::new(host, port))
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let mut config = Self::default();
        for path in config_paths()? {
            config.merge_file(&path)?;
        }
        Ok(config)
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file at {}", path.display()))?;
        let file_config: FileAppConfig = toml::from_str(&content)
            .with_context(|| format!("failed to parse config file at {}", path.display()))?;
        let mut config = Self::default();
        config.merge(file_config);
        Ok(config)
    }

    pub fn resolve_sqlite_path(&self) -> Result<PathBuf> {
        if let Some(path) = sqlite_path_env()? {
            return Ok(path);
        }

        if let Some(path) = self.backend.sqlite_path.clone() {
            return Ok(path);
        }

        if let Some(path) = self.sqlite_path.clone() {
            return Ok(path);
        }

        default_sqlite_path()
    }

    pub fn resolve_postgres_url(&self) -> Result<String> {
        if let Some(url) = postgres_url_from_parts_env()? {
            return Ok(url);
        }

        if let Some(url) = postgres_url_env()? {
            return Ok(url);
        }

        self.backend.postgres_url.clone().ok_or_else(|| {
            anyhow!("Postgres backend requires backend.postgres_url or TASKFORCE_POSTGRES_URL")
        })
    }

    pub fn resolve_postgres_ssl_root_cert(&self) -> Result<Option<PathBuf>> {
        if let Some(path) = postgres_ssl_root_cert_env()? {
            return Ok(Some(path));
        }

        Ok(self.backend.postgres_ssl_root_cert.clone())
    }

    fn merge_file(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file at {}", path.display()))?;
        let file_config: FileAppConfig = toml::from_str(&content)
            .with_context(|| format!("failed to parse config file at {}", path.display()))?;
        self.merge(file_config);
        Ok(())
    }

    fn merge(&mut self, other: FileAppConfig) {
        if other.sqlite_path.is_some() {
            self.sqlite_path = other.sqlite_path;
        }

        if let Some(backend) = other.backend {
            if let Some(kind) = backend.kind {
                self.backend.kind = kind;
            }
            if backend.sqlite_path.is_some() {
                self.backend.sqlite_path = backend.sqlite_path;
            }
            if backend.postgres_url.is_some() {
                self.backend.postgres_url = backend.postgres_url;
            }
            if backend.postgres_ssl_root_cert.is_some() {
                self.backend.postgres_ssl_root_cert = backend.postgres_ssl_root_cert;
            }
        }

        if let Some(server) = other.server {
            if server.host.is_some() {
                self.server.host = server.host;
            }
            if server.port.is_some() {
                self.server.port = server.port;
            }
        }

        if let Some(list) = other.list
            && let Some(open_statuses) = list.open_statuses
        {
            self.list.open_statuses = open_statuses;
        }
    }
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppConfig")
            .field("sqlite_path", &self.sqlite_path)
            .field("backend", &self.backend)
            .field("server", &self.server)
            .field("list", &self.list)
            .finish()
    }
}

impl fmt::Debug for BackendConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let postgres_url = self.postgres_url.as_ref().map(|_| "<redacted>");
        formatter
            .debug_struct("BackendConfig")
            .field("kind", &self.kind)
            .field("sqlite_path", &self.sqlite_path)
            .field("postgres_url", &postgres_url)
            .field("postgres_ssl_root_cert", &self.postgres_ssl_root_cert)
            .finish()
    }
}

pub fn config_path() -> Option<PathBuf> {
    config_paths().ok().and_then(|mut paths| paths.pop())
}

pub fn env_file_path() -> Option<PathBuf> {
    env_file_paths().ok().and_then(|mut paths| paths.pop())
}

pub fn base_config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("config.toml"))
}

pub fn base_env_file_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("taskforce.env"))
}

fn config_dir() -> Option<PathBuf> {
    if let Some(xdg_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg_home).join("taskforce"));
    }

    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config").join("taskforce"))
}

pub fn config_paths() -> Result<Vec<PathBuf>> {
    let Some(dir) = config_dir() else {
        return Ok(Vec::new());
    };

    let mut paths = vec![dir.join("config.toml")];
    if let Some(profile) = selected_profile()?
        && let Some(path) = profile.config_path
    {
        paths.push(path);
    }
    Ok(paths)
}

pub fn env_file_paths() -> Result<Vec<PathBuf>> {
    let Some(base_env) = base_env_file_path() else {
        return Ok(Vec::new());
    };

    let mut paths = vec![base_env];
    if let Some(profile) = selected_profile()?
        && let Some(path) = profile.env_file_path
    {
        paths.push(path);
    }
    Ok(paths)
}

pub fn current_environment() -> Option<String> {
    std::env::var("TASKFORCE_ENV")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn resolve_environment_from_sources(cli_env: Option<String>) -> Result<Option<String>> {
    if let Some(env) = cli_env.or_else(current_environment) {
        validate_environment_name(&env)?;
        return Ok(Some(env));
    }

    let Some(profiles) = load_profiles_config()? else {
        return Ok(None);
    };

    match profiles.default {
        Some(env) => {
            validate_environment_name(&env)?;
            Ok(Some(env))
        }
        None => Ok(None),
    }
}

pub fn bootstrap_environment_from_args<I, T>(args: I) -> Result<Option<String>>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString>,
{
    let mut args = args.into_iter().map(Into::into).peekable();
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy();
        if let Some(value) = text.strip_prefix("--env=") {
            let value = value.trim().to_string();
            validate_environment_name(&value)?;
            return Ok(Some(value));
        }
        if text == "--env" {
            let Some(value) = args.next() else {
                bail!("--env requires a profile name");
            };
            let value = value.to_string_lossy().trim().to_string();
            validate_environment_name(&value)?;
            return Ok(Some(value));
        }
    }

    Ok(None)
}

fn validate_environment_name(name: &str) -> Result<()> {
    if name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Ok(());
    }

    Err(anyhow!(
        "invalid TASKFORCE_ENV `{name}`: use only ASCII letters, digits, `_`, or `-`"
    ))
}

fn validate_postgres_host(host: &str) -> Result<()> {
    if host.is_empty()
        || host
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '/' || ch == '@')
    {
        return Err(anyhow!("invalid TASKFORCE_POSTGRES_HOST: {host}"));
    }

    Ok(())
}

fn percent_encode_url_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        let is_unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if is_unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

fn selected_profile() -> Result<Option<ResolvedProfile>> {
    let Some(environment) = current_environment() else {
        return Ok(None);
    };

    let profiles = load_profiles_config()?.ok_or_else(|| {
        anyhow!(
            "TASKFORCE_ENV is set to `{environment}`, but config.toml does not define [profiles]"
        )
    })?;

    let item = profiles
        .items
        .into_iter()
        .find(|item| item.id == environment)
        .ok_or_else(|| anyhow!("unknown profile `{environment}` in [profiles.items]"))?;

    Ok(Some(resolve_profile_item(item)?))
}

fn load_profiles_config() -> Result<Option<ProfilesConfig>> {
    let Some(path) = base_config_path() else {
        return Ok(None);
    };

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file at {}", path.display()))?;
    let file_config: FileAppConfig = toml::from_str(&content)
        .with_context(|| format!("failed to parse config file at {}", path.display()))?;
    Ok(file_config.profiles)
}

fn resolve_profile_item(item: ProfileItem) -> Result<ResolvedProfile> {
    validate_environment_name(&item.id)?;
    let base_dir =
        config_dir().ok_or_else(|| anyhow!("taskforce config directory is unavailable"))?;
    Ok(ResolvedProfile {
        id: item.id,
        config_path: item
            .config
            .map(|path| resolve_profile_path(&base_dir, path)),
        env_file_path: item
            .env_file
            .map(|path| resolve_profile_path(&base_dir, path)),
    })
}

fn resolve_profile_path(base_dir: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

#[derive(Debug, Clone)]
struct ResolvedProfile {
    #[allow(dead_code)]
    id: String,
    config_path: Option<PathBuf>,
    env_file_path: Option<PathBuf>,
}

pub fn sqlite_path_env() -> Result<Option<PathBuf>> {
    match std::env::var_os("TASKFORCE_SQLITE_PATH") {
        Some(path) => Ok(Some(PathBuf::from(path))),
        None => Ok(None),
    }
}

pub fn postgres_url_env() -> Result<Option<String>> {
    match std::env::var("TASKFORCE_POSTGRES_URL") {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(anyhow!("failed to read TASKFORCE_POSTGRES_URL: {err}")),
    }
}

pub fn postgres_url_from_parts_env() -> Result<Option<String>> {
    let host = match std::env::var("TASKFORCE_POSTGRES_HOST") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(err) => return Err(anyhow!("failed to read TASKFORCE_POSTGRES_HOST: {err}")),
    };
    let user = match std::env::var("TASKFORCE_POSTGRES_USER") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => "postgres".to_string(),
        Err(err) => return Err(anyhow!("failed to read TASKFORCE_POSTGRES_USER: {err}")),
    };
    let password = std::env::var("TASKFORCE_POSTGRES_PASS")
        .map_err(|err| anyhow!("failed to read TASKFORCE_POSTGRES_PASS: {err}"))?;
    let port = match std::env::var("TASKFORCE_POSTGRES_PORT") {
        Ok(value) => value
            .parse::<u16>()
            .map_err(|_| anyhow!("invalid TASKFORCE_POSTGRES_PORT: {value}"))?,
        Err(std::env::VarError::NotPresent) => 5432,
        Err(err) => return Err(anyhow!("failed to read TASKFORCE_POSTGRES_PORT: {err}")),
    };
    let database = match std::env::var("TASKFORCE_POSTGRES_DB") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => "postgres".to_string(),
        Err(err) => return Err(anyhow!("failed to read TASKFORCE_POSTGRES_DB: {err}")),
    };
    let sslmode = match std::env::var("TASKFORCE_POSTGRES_SSLMODE") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => "require".to_string(),
        Err(err) => return Err(anyhow!("failed to read TASKFORCE_POSTGRES_SSLMODE: {err}")),
    };

    validate_postgres_host(&host)?;
    Ok(Some(format!(
        "postgresql://{}:{}@{}:{}/{}?sslmode={}",
        percent_encode_url_component(&user),
        percent_encode_url_component(&password),
        host,
        port,
        percent_encode_url_component(&database),
        percent_encode_url_component(&sslmode),
    )))
}

pub fn postgres_ssl_root_cert_env() -> Result<Option<PathBuf>> {
    match std::env::var_os("TASKFORCE_POSTGRES_SSL_ROOT_CERT") {
        Some(path) => Ok(Some(PathBuf::from(path))),
        None => Ok(None),
    }
}

pub fn server_host_env() -> Option<Result<IpAddr>> {
    std::env::var("TASKFORCE_HOST").ok().map(|value| {
        IpAddr::from_str(&value).map_err(|_| anyhow!("invalid TASKFORCE_HOST: {value}"))
    })
}

pub fn server_port_env() -> Result<Option<u16>> {
    match std::env::var("TASKFORCE_PORT") {
        Ok(value) => value
            .parse::<u16>()
            .map(Some)
            .map_err(|_| anyhow!("invalid TASKFORCE_PORT: {value}")),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(anyhow!("failed to read TASKFORCE_PORT: {err}")),
    }
}

pub fn data_dir() -> Result<PathBuf> {
    if let Some(xdg_home) = std::env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(xdg_home).join("taskforce"));
    }

    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("taskforce"))
}

fn default_sqlite_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("taskforce.db"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result;

    use std::net::{IpAddr, SocketAddr};

    use super::{
        AppConfig, BackendKind, ServerConfig, bootstrap_environment_from_args, config_paths,
        env_file_paths, postgres_url_from_parts_env, resolve_environment_from_sources,
    };

    #[test]
    fn loads_sqlite_path_from_toml() -> Result<()> {
        let path = unique_temp_path("taskforce-backend-config");
        fs::write(&path, "sqlite_path = \"/tmp/taskforce.db\"\n")?;

        let config = AppConfig::load_from_path(&path)?;

        assert_eq!(config.sqlite_path, Some(PathBuf::from("/tmp/taskforce.db")));
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn loads_backend_settings_from_toml() -> Result<()> {
        let path = unique_temp_path("taskforce-backend-config");
        fs::write(
            &path,
            "[backend]\nkind = \"sqlite\"\nsqlite_path = \"/tmp/backend.db\"\n",
        )?;

        let config = AppConfig::load_from_path(&path)?;

        assert_eq!(config.backend.kind, BackendKind::Sqlite);
        assert_eq!(
            config.backend.sqlite_path,
            Some(PathBuf::from("/tmp/backend.db"))
        );
        assert_eq!(
            config.resolve_sqlite_path()?,
            PathBuf::from("/tmp/backend.db")
        );
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn loads_postgres_backend_settings_from_toml() -> Result<()> {
        let path = unique_temp_path("taskforce-postgres-config");
        fs::write(
            &path,
            "[backend]\nkind = \"postgres\"\npostgres_url = \"postgresql://user:pass@db.example.com/postgres?sslmode=require\"\npostgres_ssl_root_cert = \"/tmp/supabase-prod-ca-2021.crt\"\n",
        )?;

        let config = AppConfig::load_from_path(&path)?;

        assert_eq!(config.backend.kind, BackendKind::Postgres);
        assert_eq!(
            config.backend.postgres_url.as_deref(),
            Some("postgresql://user:pass@db.example.com/postgres?sslmode=require")
        );
        assert_eq!(
            config.resolve_postgres_url()?,
            "postgresql://user:pass@db.example.com/postgres?sslmode=require"
        );
        assert_eq!(
            config.resolve_postgres_ssl_root_cert()?,
            Some(PathBuf::from("/tmp/supabase-prod-ca-2021.crt"))
        );
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn overlays_environment_specific_config() -> Result<()> {
        let _guard = env_lock().lock().expect("env lock");
        let dir = unique_temp_dir("taskforce-config-env");
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &dir);
            std::env::set_var("TASKFORCE_ENV", "development");
        }

        let taskforce_dir = dir.join("taskforce");
        fs::create_dir_all(&taskforce_dir)?;
        fs::write(
            taskforce_dir.join("config.toml"),
            "[profiles]\ndefault = \"production\"\n[[profiles.items]]\nid = \"development\"\nconfig = \"config.development.toml\"\n[[profiles.items]]\nid = \"production\"\nconfig = \"config.production.toml\"\n[backend]\nkind = \"sqlite\"\n[server]\nport = 9090\n",
        )?;
        fs::write(
            taskforce_dir.join("config.development.toml"),
            "[backend]\nkind = \"postgres\"\npostgres_ssl_root_cert = \"/tmp/dev.crt\"\n",
        )?;

        let config = AppConfig::load()?;
        assert_eq!(config.backend.kind, BackendKind::Postgres);
        assert_eq!(
            config.backend.postgres_ssl_root_cert,
            Some(PathBuf::from("/tmp/dev.crt"))
        );
        assert_eq!(config.server.port, Some(9090));

        unsafe {
            std::env::remove_var("TASKFORCE_ENV");
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn builds_environment_specific_paths() -> Result<()> {
        let _guard = env_lock().lock().expect("env lock");
        let dir = unique_temp_dir("taskforce-config-paths");
        fs::create_dir_all(&dir)?;
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &dir);
            std::env::set_var("TASKFORCE_ENV", "production");
        }
        let taskforce_dir = dir.join("taskforce");
        fs::create_dir_all(&taskforce_dir)?;
        fs::write(
            taskforce_dir.join("config.toml"),
            "[profiles]\ndefault = \"production\"\n[[profiles.items]]\nid = \"production\"\nconfig = \"config.production.toml\"\nenv_file = \"production.env\"\n",
        )?;

        let config_files = config_paths()?;
        let env_files = env_file_paths()?;
        assert_eq!(config_files.len(), 2);
        assert!(config_files[0].ends_with("taskforce/config.toml"));
        assert!(config_files[1].ends_with("taskforce/config.production.toml"));
        assert_eq!(env_files.len(), 2);
        assert!(env_files[0].ends_with("taskforce/taskforce.env"));
        assert!(env_files[1].ends_with("taskforce/production.env"));

        unsafe {
            std::env::remove_var("TASKFORCE_ENV");
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn parses_environment_from_cli_args() -> Result<()> {
        assert_eq!(
            bootstrap_environment_from_args(["taskforce", "--env=development", "list"])?,
            Some("development".to_string())
        );
        assert_eq!(
            bootstrap_environment_from_args(["taskforce", "--env", "production", "serve"])?,
            Some("production".to_string())
        );
        Ok(())
    }

    #[test]
    fn resolves_default_environment_from_profiles() -> Result<()> {
        let _guard = env_lock().lock().expect("env lock");
        let dir = unique_temp_dir("taskforce-config-default-env");
        let taskforce_dir = dir.join("taskforce");
        fs::create_dir_all(&taskforce_dir)?;
        fs::write(
            taskforce_dir.join("config.toml"),
            "[profiles]\ndefault = \"production\"\n[[profiles.items]]\nid = \"production\"\nenv_file = \"production.env\"\n",
        )?;
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &dir);
            std::env::remove_var("TASKFORCE_ENV");
        }

        assert_eq!(
            resolve_environment_from_sources(None)?,
            Some("production".to_string())
        );

        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn redacts_postgres_url_in_debug_output() {
        let config = AppConfig {
            sqlite_path: None,
            backend: super::BackendConfig {
                kind: BackendKind::Postgres,
                sqlite_path: None,
                postgres_url: Some("postgresql://postgres:secret@db.example.com/postgres".into()),
                postgres_ssl_root_cert: Some(PathBuf::from("/tmp/supabase-prod-ca-2021.crt")),
            },
            server: ServerConfig::default(),
            list: super::ListConfig::default(),
        };

        let debug = format!("{config:?}");
        assert!(debug.contains("postgres_url: Some(\"<redacted>\")"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn builds_postgres_url_from_split_environment_variables() -> Result<()> {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::set_var("TASKFORCE_POSTGRES_HOST", "db.example.com");
            std::env::set_var("TASKFORCE_POSTGRES_USER", "postgres");
            std::env::set_var("TASKFORCE_POSTGRES_PASS", "ab@cd:ef");
            std::env::set_var("TASKFORCE_POSTGRES_PORT", "5432");
            std::env::set_var("TASKFORCE_POSTGRES_DB", "postgres");
            std::env::remove_var("TASKFORCE_POSTGRES_SSLMODE");
        }

        let url = postgres_url_from_parts_env()?.expect("url");
        assert_eq!(
            url,
            "postgresql://postgres:ab%40cd%3Aef@db.example.com:5432/postgres?sslmode=require"
        );

        unsafe {
            std::env::remove_var("TASKFORCE_POSTGRES_HOST");
            std::env::remove_var("TASKFORCE_POSTGRES_USER");
            std::env::remove_var("TASKFORCE_POSTGRES_PASS");
            std::env::remove_var("TASKFORCE_POSTGRES_PORT");
            std::env::remove_var("TASKFORCE_POSTGRES_DB");
        }
        Ok(())
    }

    #[test]
    fn loads_server_settings_from_toml() -> Result<()> {
        let path = unique_temp_path("taskforce-server-config");
        fs::write(&path, "[server]\nhost = \"0.0.0.0\"\nport = 9090\n")?;

        let config = AppConfig::load_from_path(&path)?;

        assert_eq!(config.server.host, Some(IpAddr::from([0, 0, 0, 0])));
        assert_eq!(config.server.port, Some(9090));
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn resolves_server_addr_with_defaults() -> Result<()> {
        let resolved = ServerConfig::default().resolve()?;
        assert_eq!(resolved, SocketAddr::from(([127, 0, 0, 1], 0)));
        Ok(())
    }

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}.toml"))
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}"))
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
