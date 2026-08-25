use crate::{
    quantum_crypto::{PKTrait, SKTrait},
    QuantumKeyPair,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use zeroize::Zeroize;

const KEY_FILE_NAME: &str = "validator_dilithium5_key.json";
const DEFAULT_PUBLIC_KEY_FILE_NAME: &str = "DILITHIUM_PUB_KEY.hex";
const KEY_FILE_VERSION: u32 = 1;
const KEYPAIR_PROBE: &[u8] = b"ULTRANET_VALIDATOR_KEYPAIR_CHECK_V1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedPublicKey {
    pub path: PathBuf,
    pub address: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredValidatorKey {
    version: u32,
    address: String,
    public_key: String,
    secret_key: String,
}

impl StoredValidatorKey {
    fn clear_sensitive(&mut self) {
        self.address.zeroize();
        self.public_key.zeroize();
        self.secret_key.zeroize();
    }
}

struct ValidatorIdentity {
    keypair: QuantumKeyPair,
}

/// Ensure the node has a stable local Dilithium identity and return its public address.
/// The secret key is loaded only for validation and is dropped before this function returns.
pub fn ensure(db_path: &Path) -> Result<String, String> {
    let identity = load_or_create(db_path)?;
    Ok(identity.keypair.address())
}

/// Export only the persistent validator public key as lowercase hexadecimal.
/// The corresponding secret key is never written to the requested output path.
pub fn export_public_key(
    db_path: &Path,
    output_path: Option<&Path>,
) -> Result<ExportedPublicKey, String> {
    let identity = load_or_create(db_path)?;
    let address = identity.keypair.address();
    let public_key = hex::encode(&identity.keypair.public_key);
    drop(identity);

    let path = output_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_PUBLIC_KEY_FILE_NAME));
    write_new_file(&path, &format!("{public_key}\n"), false)?;

    Ok(ExportedPublicKey { path, address })
}

fn load_or_create(db_path: &Path) -> Result<ValidatorIdentity, String> {
    fs::create_dir_all(db_path).map_err(|error| {
        format!(
            "Unable to create the node data directory {}: {error}",
            db_path.display()
        )
    })?;

    let key_path = db_path.join(KEY_FILE_NAME);
    match fs::symlink_metadata(&key_path) {
        Ok(_) => {
            validate_private_key_file(&key_path)?;
            load_identity(&key_path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => create_identity(&key_path),
        Err(error) => Err(format!(
            "Unable to inspect the validator key file {}: {error}",
            key_path.display()
        )),
    }
}

fn create_identity(key_path: &Path) -> Result<ValidatorIdentity, String> {
    let keypair = QuantumKeyPair::generate();
    let mut record = StoredValidatorKey {
        version: KEY_FILE_VERSION,
        address: keypair.address(),
        public_key: hex::encode(&keypair.public_key),
        secret_key: hex::encode(&keypair.secret_key),
    };

    let mut serialized = match serde_json::to_string_pretty(&record) {
        Ok(value) => value,
        Err(error) => {
            record.clear_sensitive();
            return Err(format!("Unable to encode the local validator key: {error}"));
        }
    };
    record.clear_sensitive();

    let write_result = write_new_file(key_path, &serialized, true);
    serialized.zeroize();
    write_result?;

    Ok(ValidatorIdentity { keypair })
}

fn load_identity(key_path: &Path) -> Result<ValidatorIdentity, String> {
    let mut raw = fs::read_to_string(key_path).map_err(|error| {
        format!(
            "Unable to read the local validator key file {}: {error}",
            key_path.display()
        )
    })?;
    let mut record: StoredValidatorKey = match serde_json::from_str(&raw) {
        Ok(record) => record,
        Err(error) => {
            raw.zeroize();
            return Err(format!(
                "The local validator key file {} is invalid: {error}",
                key_path.display()
            ));
        }
    };
    raw.zeroize();
    let stored_address = record.address.clone();

    if record.version != KEY_FILE_VERSION {
        record.clear_sensitive();
        return Err(format!(
            "The local validator key file {} uses unsupported version {}",
            key_path.display(),
            record.version
        ));
    }

    let public_key = hex::decode(record.public_key.trim()).map_err(|error| {
        record.clear_sensitive();
        format!(
            "The public key in {} is not valid hexadecimal: {error}",
            key_path.display()
        )
    })?;
    let mut secret_key = hex::decode(record.secret_key.trim()).map_err(|error| {
        record.clear_sensitive();
        format!(
            "The secret key in {} is not valid hexadecimal: {error}",
            key_path.display()
        )
    })?;
    record.clear_sensitive();

    if public_key.len() != crate::quantum_crypto::public_key_bytes() {
        secret_key.zeroize();
        return Err(format!(
            "The validator public key must contain {} bytes",
            crate::quantum_crypto::public_key_bytes()
        ));
    }
    if secret_key.len() != crate::quantum_crypto::secret_key_bytes() {
        secret_key.zeroize();
        return Err(format!(
            "The validator secret key must contain {} bytes",
            crate::quantum_crypto::secret_key_bytes()
        ));
    }

    if crate::quantum_crypto::PublicKey::from_bytes(&public_key).is_err() {
        secret_key.zeroize();
        return Err("The local validator public key is not a Dilithium-5 key".to_string());
    }
    if crate::quantum_crypto::SecretKey::from_bytes(&secret_key).is_err() {
        secret_key.zeroize();
        return Err("The local validator secret key is not a Dilithium-5 key".to_string());
    }

    let expected_address = QuantumKeyPair::address_from_public_key(&public_key);
    if stored_address != expected_address {
        secret_key.zeroize();
        return Err("The local validator key address does not match its public key".to_string());
    }

    let keypair = QuantumKeyPair {
        public_key,
        secret_key,
        key_id: [0; 32],
        created_at: 0,
        version: 1,
    };
    let mut signature = keypair.sign(KEYPAIR_PROBE);
    let valid = QuantumKeyPair::verify(&keypair.public_key, KEYPAIR_PROBE, &signature);
    signature.zeroize();
    if !valid {
        return Err("The local validator key failed its signing self-check".to_string());
    }

    Ok(ValidatorIdentity { keypair })
}

fn validate_private_key_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Unable to inspect the validator key file {}: {error}",
            path.display()
        )
    })?;
    if !metadata.file_type().is_file() {
        return Err(format!(
            "The validator key path {} must be a regular private file",
            path.display()
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(format!(
                "The validator key file {} is readable by group or other users; restrict it to the owner",
                path.display()
            ));
        }
    }

    Ok(())
}

fn write_new_file(path: &Path, contents: &str, private: bool) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err(format!(
                "The output directory {} does not exist",
                parent.display()
            ));
        }
    }

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = private;

    let mut file = options.open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            format!(
                "Refusing to overwrite existing file {}. Choose a new output path or verify the existing identity first",
                path.display()
            )
        } else {
            format!("Unable to create output file {}: {error}", path.display())
        }
    })?;

    if let Err(error) = file
        .write_all(contents.as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!(
            "Unable to write output file {}: {error}",
            path.display()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(label: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ultranet-validator-{label}-{}-{timestamp}",
            std::process::id()
        ))
    }

    #[test]
    fn identity_is_stable_across_loads() {
        let directory = temporary_directory("stable");
        let first = load_or_create(&directory).unwrap();
        let expected_address = first.keypair.address();
        let expected_public_key = first.keypair.public_key.clone();
        drop(first);

        let second = load_or_create(&directory).unwrap();
        assert_eq!(second.keypair.address(), expected_address);
        assert_eq!(second.keypair.public_key, expected_public_key);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(directory.join(KEY_FILE_NAME))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0);
        }

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn export_writes_only_the_public_hex_file_and_refuses_overwrite() {
        let directory = temporary_directory("export");
        let output = directory.join(DEFAULT_PUBLIC_KEY_FILE_NAME);
        let identity = load_or_create(&directory).unwrap();
        let expected_public_key = hex::encode(&identity.keypair.public_key);
        let expected_address = identity.keypair.address();
        drop(identity);

        let exported = export_public_key(&directory, Some(&output)).unwrap();
        assert_eq!(exported.address, expected_address);
        assert_eq!(
            fs::read_to_string(&output).unwrap().trim(),
            expected_public_key
        );
        assert!(export_public_key(&directory, Some(&output)).is_err());

        let _ = fs::remove_dir_all(directory);
    }
}
