//! Cross-platform runtime configuration for local and service launches.
//!
//! The node keeps authentication mandatory, but makes the first-run failure
//! actionable and gives desktop launches a writable, predictable data path.

use std::{
    env, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

const DESKTOP_ENV_FILE: &str = "UltraNetNode.env";
const ENV_FILE_OVERRIDE: &str = "ULTRANET_ENV_FILE";
const DB_PATH_ENV: &str = "ULTRANET_DB_PATH";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub db_path: PathBuf,
    pub env_file: Option<PathBuf>,
}

/// Load the optional desktop env file, resolve the data path, and validate all
/// startup configuration before storage and cryptographic initialization.
pub fn prepare() -> Result<RuntimeConfig, String> {
    let env_file = load_optional_env_file()?;
    let db_path = resolve_db_path()?;
    env::set_var(DB_PATH_ENV, &db_path);

    crate::api::validate_configuration().map_err(|error| {
        let message = error.to_string();
        format!(
            "Configuration error: {}. Check UltraNetNode.env or the service environment and start the node again.",
            message.trim_end_matches('.')
        )
    })?;

    Ok(RuntimeConfig { db_path, env_file })
}

pub fn check_config() -> Result<(), String> {
    let _ = prepare()?;
    Ok(())
}

pub fn pause_on_error() -> bool {
    match env::var("ULTRANET_PAUSE_ON_ERROR") {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        ),
        Err(env::VarError::NotPresent) => cfg!(windows),
        Err(env::VarError::NotUnicode(_)) => false,
    }
}

fn load_optional_env_file() -> Result<Option<PathBuf>, String> {
    let Some(path) = discover_env_file()? else {
        return Ok(None);
    };

    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Cannot read environment file {}: {error}", path.display()))?;

    for (line_number, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            return Err(format!(
                "Invalid environment file {} at line {}: expected KEY=value",
                path.display(),
                line_number + 1
            ));
        };
        let key = raw_key.trim();
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(format!(
                "Invalid environment file {} at line {}: keys must use uppercase letters, digits, and underscores",
                path.display(),
                line_number + 1
            ));
        }

        // Process/systemd/container variables always win over the optional
        // sibling file. Never echo the value because it may be a secret token.
        if env::var_os(key).is_none() {
            env::set_var(key, raw_value.trim());
        }
    }

    Ok(Some(path))
}

fn discover_env_file() -> Result<Option<PathBuf>, String> {
    if let Some(path) = env::var_os(ENV_FILE_OVERRIDE) {
        let path = PathBuf::from(path);
        if !path.is_file() {
            return Err(format!(
                "The environment file configured by ULTRANET_ENV_FILE does not exist: {}",
                path.display()
            ));
        }
        return Ok(Some(path));
    }

    let mut candidates = Vec::new();
    if let Ok(executable) = env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.join(DESKTOP_ENV_FILE));
        }
    }
    if let Ok(current_dir) = env::current_dir() {
        candidates.push(current_dir.join(DESKTOP_ENV_FILE));
    }

    Ok(candidates.into_iter().find(|path| path.is_file()))
}

fn resolve_db_path() -> Result<PathBuf, String> {
    if let Some(raw) = env::var_os(DB_PATH_ENV) {
        let raw = raw.to_string_lossy().trim().to_string();
        if raw.is_empty() {
            return Err("ULTRANET_DB_PATH cannot be empty".to_string());
        }
        return ensure_data_directory(PathBuf::from(raw));
    }

    ensure_data_directory(default_data_path())
}

fn ensure_data_directory(path: PathBuf) -> Result<PathBuf, String> {
    fs::create_dir_all(&path).map_err(|error| {
        format!(
            "Cannot create node data directory {}: {error}",
            path.display()
        )
    })?;
    Ok(path)
}

fn default_data_path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local_app_data).join("UltraNet").join("data");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("UltraNet")
                .join("data");
        }
    }

    #[cfg(not(windows))]
    if let Some(xdg_data_home) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg_data_home).join("ultranet");
    }

    #[cfg(not(windows))]
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("ultranet");
    }

    Path::new("ultranet_db").to_path_buf()
}

#[allow(dead_code)]
fn _io_error(kind: ErrorKind, message: impl Into<String>) -> io::Error {
    io::Error::new(kind, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{Mutex, OnceLock},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        env::temp_dir().join(format!(
            "ultranet-runtime-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn env_file_fills_missing_values_without_overwriting_process_values() {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");
        let directory = temporary_directory("env");
        fs::create_dir_all(&directory).unwrap();
        let env_file = directory.join(DESKTOP_ENV_FILE);
        fs::write(
            &env_file,
            "ULTRANET_RUNTIME_TEST_FILE=value-from-file\nULTRANET_RUNTIME_TEST_PRECEDENCE=file-value\n",
        )
        .unwrap();
        env::set_var(ENV_FILE_OVERRIDE, &env_file);
        env::set_var("ULTRANET_RUNTIME_TEST_PRECEDENCE", "process-value");

        load_optional_env_file().unwrap();

        assert_eq!(
            env::var("ULTRANET_RUNTIME_TEST_FILE").unwrap(),
            "value-from-file"
        );
        assert_eq!(
            env::var("ULTRANET_RUNTIME_TEST_PRECEDENCE").unwrap(),
            "process-value"
        );

        env::remove_var(ENV_FILE_OVERRIDE);
        env::remove_var("ULTRANET_RUNTIME_TEST_FILE");
        env::remove_var("ULTRANET_RUNTIME_TEST_PRECEDENCE");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn malformed_env_file_reports_line_number_without_exposing_values() {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");
        let directory = temporary_directory("malformed");
        fs::create_dir_all(&directory).unwrap();
        let env_file = directory.join(DESKTOP_ENV_FILE);
        fs::write(
            &env_file,
            "ULTRANET_RUNTIME_SECRET=secret-value\nnot-a-setting\n",
        )
        .unwrap();
        env::set_var(ENV_FILE_OVERRIDE, &env_file);

        let error = load_optional_env_file().unwrap_err();
        assert!(error.contains("line 2"));
        assert!(!error.contains("secret-value"));

        env::remove_var(ENV_FILE_OVERRIDE);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn explicit_db_path_is_created_and_returned() {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");
        let directory = temporary_directory("db");
        env::set_var(DB_PATH_ENV, &directory);

        let resolved = resolve_db_path().unwrap();
        assert_eq!(resolved, directory);
        assert!(resolved.is_dir());

        env::remove_var(DB_PATH_ENV);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn empty_explicit_db_path_is_rejected() {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");
        env::set_var(DB_PATH_ENV, "   ");

        let error = resolve_db_path().unwrap_err();
        assert_eq!(error, "ULTRANET_DB_PATH cannot be empty");

        env::remove_var(DB_PATH_ENV);
    }

    #[cfg(windows)]
    #[test]
    fn windows_default_data_path_uses_localappdata() {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");
        let previous = env::var_os("LOCALAPPDATA");
        let local_app_data = temporary_directory("localappdata");
        env::set_var("LOCALAPPDATA", &local_app_data);

        assert_eq!(
            default_data_path(),
            local_app_data.join("UltraNet").join("data")
        );

        match previous {
            Some(value) => env::set_var("LOCALAPPDATA", value),
            None => env::remove_var("LOCALAPPDATA"),
        }
    }
}
