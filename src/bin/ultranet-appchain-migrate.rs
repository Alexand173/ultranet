use clap::Parser;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use UltraNet::appchain::{
    derive_appchain_treasury_address, AnchoredState, AppChainConfig, DEFAULT_APPCHAIN_ANCHOR_FEE,
};

const BACKUP_FORMAT_VERSION: u32 = 1;
const MIGRATION_TARGET_VERSION: u32 = 2;

/// Offline migration for the pre-production AppChain Sled records.
///
/// The command always writes a raw backup first. It is a dry run unless
/// `--apply` is provided, and it refuses to touch a database while the node is
/// running so an operator cannot migrate a live Sled instance accidentally.
#[derive(Debug, Parser)]
#[command(name = "ultranet-appchain-migrate")]
#[command(about = "Back up and migrate legacy AppChain registry records")]
struct Args {
    /// Sled database directory used by the node.
    #[arg(long, value_name = "PATH")]
    db_path: PathBuf,

    /// New, empty directory where raw registry records and a manifest are saved.
    #[arg(long, value_name = "PATH")]
    backup_dir: PathBuf,

    /// Replace legacy records after the backup and validation steps succeed.
    #[arg(long)]
    apply: bool,
}

#[derive(Debug, Clone)]
struct RawRecord {
    key: Vec<u8>,
    value: Vec<u8>,
}

#[derive(Debug, Serialize)]
struct BackupRecord {
    key_hex: String,
    value_hex: String,
}

#[derive(Debug, Serialize)]
struct BackupManifest {
    backup_format_version: u32,
    migration_target_version: u32,
    created_at_unix: u64,
    database_path: String,
    config_record_count: usize,
    anchor_record_count: usize,
    configs_digest_sha3_256: String,
    anchors_digest_sha3_256: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacyAppChainConfig {
    id: u32,
    name: String,
    owner: String,
    genesis_root: [u8; 32],
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacyAnchoredState {
    chain_id: u32,
    state_root: String,
    proof: String,
    timestamp: u64,
}

#[derive(Debug)]
struct MigrationPlan {
    configs: Vec<AppChainConfig>,
    anchors: Vec<AnchoredState>,
    legacy_config_count: usize,
    legacy_anchor_count: usize,
    repaired_config_count: usize,
    repaired_anchor_key_count: usize,
    changed: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("AppChain registry migration failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    ensure_database_path(&args.db_path)?;
    ensure_node_stopped()?;

    let db = sled::open(&args.db_path).map_err(|error| {
        format!(
            "unable to open Sled database {}: {error}",
            args.db_path.display()
        )
    })?;
    let config_tree = db
        .open_tree("appchain_configs")
        .map_err(|error| format!("unable to open appchain_configs tree: {error}"))?;
    let anchor_tree = db
        .open_tree("appchain_anchors")
        .map_err(|error| format!("unable to open appchain_anchors tree: {error}"))?;

    let raw_configs = read_records(&config_tree, "appchain_configs")?;
    let raw_anchors = read_records(&anchor_tree, "appchain_anchors")?;
    write_backup(&args.backup_dir, &args.db_path, &raw_configs, &raw_anchors)?;

    let plan = build_plan(&raw_configs, &raw_anchors)?;
    print_plan(&args, &plan, &args.backup_dir);

    if !args.apply {
        println!("Dry run only. Re-run with --apply to replace the registry records.");
        return Ok(());
    }

    if !plan.changed {
        println!("No registry migration is required; the backed-up records are already current.");
        return Ok(());
    }

    apply_plan(
        &db,
        &config_tree,
        &anchor_tree,
        &raw_configs,
        &raw_anchors,
        &plan,
    )?;
    println!("AppChain registry migration applied successfully.");
    Ok(())
}

fn ensure_database_path(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Err(format!(
            "database path does not exist or is not a directory: {}",
            path.display()
        ));
    }
    Ok(())
}

fn ensure_node_stopped() -> Result<(), String> {
    if let Ok(status) = Command::new("systemctl")
        .args(["is-active", "--quiet", "ultranet.service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        if status.success() {
            return Err(
                "ultranet.service is active; stop it before running the migration".to_string(),
            );
        }
    }

    if let Ok(status) = Command::new("pgrep")
        .args(["-x", "UltraNet"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        if status.success() {
            return Err(
                "an UltraNet node process is still running; stop it before migrating".to_string(),
            );
        }
    }

    Ok(())
}

fn read_records(tree: &sled::Tree, tree_name: &str) -> Result<Vec<RawRecord>, String> {
    tree.iter()
        .map(|item| {
            let (key, value) =
                item.map_err(|error| format!("unable to read {tree_name} record: {error}"))?;
            Ok(RawRecord {
                key: key.to_vec(),
                value: value.to_vec(),
            })
        })
        .collect()
}

fn write_backup(
    backup_dir: &Path,
    db_path: &Path,
    configs: &[RawRecord],
    anchors: &[RawRecord],
) -> Result<(), String> {
    if backup_dir.exists() {
        let mut entries = fs::read_dir(backup_dir)
            .map_err(|error| format!("unable to inspect backup directory: {error}"))?;
        if entries.next().is_some() {
            return Err(format!(
                "backup directory must be new and empty: {}",
                backup_dir.display()
            ));
        }
    } else {
        fs::create_dir_all(backup_dir).map_err(|error| {
            format!(
                "unable to create backup directory {}: {error}",
                backup_dir.display()
            )
        })?;
    }

    write_jsonl(&backup_dir.join("appchain_configs.jsonl"), configs)?;
    write_jsonl(&backup_dir.join("appchain_anchors.jsonl"), anchors)?;

    let manifest = BackupManifest {
        backup_format_version: BACKUP_FORMAT_VERSION,
        migration_target_version: MIGRATION_TARGET_VERSION,
        created_at_unix: unix_timestamp()?,
        database_path: db_path.display().to_string(),
        config_record_count: configs.len(),
        anchor_record_count: anchors.len(),
        configs_digest_sha3_256: records_digest(configs),
        anchors_digest_sha3_256: records_digest(anchors),
    };
    let manifest_path = backup_dir.join("manifest.json");
    let manifest_json = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("unable to encode backup manifest: {error}"))?;
    fs::write(&manifest_path, manifest_json)
        .map_err(|error| format!("unable to write {}: {error}", manifest_path.display()))?;

    println!(
        "Raw AppChain registry backup written to {}",
        backup_dir.display()
    );
    Ok(())
}

fn write_jsonl(path: &Path, records: &[RawRecord]) -> Result<(), String> {
    let file = File::create(path)
        .map_err(|error| format!("unable to create {}: {error}", path.display()))?;
    let mut writer = BufWriter::new(file);
    for record in records {
        let line = serde_json::to_string(&BackupRecord {
            key_hex: hex::encode(&record.key),
            value_hex: hex::encode(&record.value),
        })
        .map_err(|error| format!("unable to encode backup record: {error}"))?;
        writeln!(writer, "{line}")
            .map_err(|error| format!("unable to write {}: {error}", path.display()))?;
    }
    writer
        .flush()
        .map_err(|error| format!("unable to flush {}: {error}", path.display()))
}

fn records_digest(records: &[RawRecord]) -> String {
    let mut hasher = Sha3_256::new();
    for record in records {
        hasher.update((record.key.len() as u64).to_le_bytes());
        hasher.update(&record.key);
        hasher.update((record.value.len() as u64).to_le_bytes());
        hasher.update(&record.value);
    }
    hex::encode(hasher.finalize())
}

fn build_plan(
    config_records: &[RawRecord],
    anchor_records: &[RawRecord],
) -> Result<MigrationPlan, String> {
    let mut configs = BTreeMap::<u32, AppChainConfig>::new();
    let mut legacy_config_count = 0;
    let mut repaired_config_count = 0;

    for record in config_records {
        if record.key.len() != 4 {
            return Err("AppChain config key must be exactly 4 bytes".to_string());
        }
        let key_id = u32::from_be_bytes(record.key.as_slice().try_into().unwrap());
        let (mut config, was_legacy) = decode_config(&record.value)?;
        if config.id != key_id {
            return Err(format!(
                "AppChain config key {} does not match record id {}",
                key_id, config.id
            ));
        }
        if was_legacy {
            legacy_config_count += 1;
        }
        let expected_treasury = derive_appchain_treasury_address(config.id);
        if config.account_address != expected_treasury {
            config.account_address = expected_treasury;
            repaired_config_count += 1;
        }
        if configs.insert(config.id, config).is_some() {
            return Err(format!("duplicate AppChain config id {key_id}"));
        }
    }

    let mut anchors = Vec::new();
    let mut legacy_anchors = Vec::new();
    let mut next_anchor_number = HashMap::<u32, u64>::new();
    let mut repaired_anchor_key_count = 0;

    for (index, record) in anchor_records.iter().enumerate() {
        match bincode::deserialize::<AnchoredState>(&record.value) {
            Ok(anchor) => {
                if !configs.contains_key(&anchor.chain_id) {
                    return Err(format!(
                        "AppChain anchor references missing AppChain #{}",
                        anchor.chain_id
                    ));
                }
                let expected_key = anchor_storage_key(&anchor);
                if record.key != expected_key {
                    repaired_anchor_key_count += 1;
                }
                next_anchor_number
                    .entry(anchor.chain_id)
                    .and_modify(|number| *number = (*number).max(anchor.anchor_number))
                    .or_insert(anchor.anchor_number);
                anchors.push(anchor);
            }
            Err(current_error) => {
                match bincode::deserialize::<LegacyAnchoredState>(&record.value) {
                    Ok(anchor) => {
                        if !configs.contains_key(&anchor.chain_id) {
                            return Err(format!(
                                "legacy AppChain anchor references missing AppChain #{}",
                                anchor.chain_id
                            ));
                        }
                        legacy_anchors.push((index, anchor));
                    }
                    Err(legacy_error) => {
                        return Err(format!(
                        "unable to decode AppChain anchor record {} as current ({current_error}) or legacy ({legacy_error})",
                        hex::encode(&record.key)
                    ));
                    }
                }
            }
        }
    }

    let legacy_anchor_count = legacy_anchors.len();
    legacy_anchors.sort_by_key(|(index, anchor)| (anchor.chain_id, anchor.timestamp, *index));
    for (_index, legacy) in legacy_anchors {
        let next_number = next_anchor_number.entry(legacy.chain_id).or_insert(0);
        *next_number = next_number.checked_add(1).ok_or_else(|| {
            format!(
                "AppChain #{} anchor number overflowed during migration",
                legacy.chain_id
            )
        })?;
        let anchor_number = *next_number;
        anchors.push(AnchoredState {
            chain_id: legacy.chain_id,
            anchor_number,
            state_root: legacy.state_root,
            proof: legacy.proof,
            timestamp: legacy.timestamp,
            fee_charged: 0,
            // The legacy endpoint had no treasury debit contract. Preserve it
            // as historical test data rather than inventing a production fee.
            is_test: true,
        });
    }

    anchors.sort_by_key(|anchor| (anchor.timestamp, anchor.chain_id, anchor.anchor_number));

    let mut stats_changed = false;
    for config in configs.values_mut() {
        let chain_anchors: Vec<&AnchoredState> = anchors
            .iter()
            .filter(|anchor| anchor.chain_id == config.id)
            .collect();
        if let Some(max_number) = chain_anchors
            .iter()
            .map(|anchor| anchor.anchor_number)
            .max()
        {
            if config.anchor_count < max_number {
                config.anchor_count = max_number;
                stats_changed = true;
            }
        }
        if let Some(latest) = chain_anchors
            .iter()
            .max_by_key(|anchor| (anchor.timestamp, anchor.anchor_number))
        {
            if config
                .latest_anchor_at
                .map_or(true, |timestamp| timestamp < latest.timestamp)
            {
                config.latest_anchor_at = Some(latest.timestamp);
                config.latest_state_root = Some(latest.state_root.clone());
                stats_changed = true;
            }
        }
    }

    let changed = legacy_config_count > 0
        || legacy_anchor_count > 0
        || repaired_config_count > 0
        || repaired_anchor_key_count > 0
        || stats_changed;

    Ok(MigrationPlan {
        configs: configs.into_values().collect(),
        anchors,
        legacy_config_count,
        legacy_anchor_count,
        repaired_config_count,
        repaired_anchor_key_count,
        changed,
    })
}

fn decode_config(value: &[u8]) -> Result<(AppChainConfig, bool), String> {
    match bincode::deserialize::<AppChainConfig>(value) {
        Ok(config) => Ok((config, false)),
        Err(current_error) => match bincode::deserialize::<LegacyAppChainConfig>(value) {
            Ok(legacy) => Ok((
                AppChainConfig {
                    id: legacy.id,
                    name: legacy.name,
                    owner: legacy.owner,
                    account_address: derive_appchain_treasury_address(legacy.id),
                    genesis_root: legacy.genesis_root,
                    anchor_fee: DEFAULT_APPCHAIN_ANCHOR_FEE,
                    anchor_spend: 0,
                    anchor_count: 0,
                    latest_anchor_at: None,
                    latest_state_root: None,
                },
                true,
            )),
            Err(legacy_error) => Err(format!(
                "unable to decode AppChain config as current ({current_error}) or legacy ({legacy_error})"
            )),
        },
    }
}

fn anchor_storage_key(anchor: &AnchoredState) -> [u8; 32] {
    let value = bincode::serialize(anchor).expect("AnchoredState serialization cannot fail");
    Sha3_256::digest(value).into()
}

fn apply_plan(
    db: &sled::Db,
    config_tree: &sled::Tree,
    anchor_tree: &sled::Tree,
    old_configs: &[RawRecord],
    old_anchors: &[RawRecord],
    plan: &MigrationPlan,
) -> Result<(), String> {
    let new_configs = plan
        .configs
        .iter()
        .map(|config| RawRecord {
            key: config.id.to_be_bytes().to_vec(),
            value: bincode::serialize(config).expect("AppChainConfig serialization cannot fail"),
        })
        .collect::<Vec<_>>();
    let new_anchors = plan
        .anchors
        .iter()
        .map(|anchor| {
            let value =
                bincode::serialize(anchor).expect("AnchoredState serialization cannot fail");
            RawRecord {
                key: anchor_storage_key(anchor).to_vec(),
                value,
            }
        })
        .collect::<Vec<_>>();

    if let Err(error) = replace_tree(config_tree, &new_configs)
        .and_then(|_| replace_tree(anchor_tree, &new_anchors))
        .and_then(|_| db.flush().map_err(|flush_error| flush_error.to_string()))
    {
        eprintln!("Migration write failed; attempting to restore raw registry records: {error}");
        let restore_result = replace_tree(config_tree, old_configs)
            .and_then(|_| replace_tree(anchor_tree, old_anchors))
            .and_then(|_| db.flush().map_err(|flush_error| flush_error.to_string()));
        if let Err(restore_error) = restore_result {
            return Err(format!(
                "{error}; automatic restore also failed: {restore_error}. Restore from the raw backup immediately."
            ));
        }
        return Err(format!("{error}; original registry records were restored"));
    }

    Ok(())
}

fn replace_tree(tree: &sled::Tree, records: &[RawRecord]) -> Result<(), String> {
    tree.clear().map_err(|error| error.to_string())?;
    let mut batch = sled::Batch::default();
    for record in records {
        batch.insert(record.key.as_slice(), record.value.as_slice());
    }
    tree.apply_batch(batch).map_err(|error| error.to_string())
}

fn print_plan(args: &Args, plan: &MigrationPlan, backup_dir: &Path) {
    println!("Migration target schema: v{MIGRATION_TARGET_VERSION}");
    println!("  Config records: {}", plan.configs.len());
    println!("  Anchor records: {}", plan.anchors.len());
    println!("  Legacy configs converted: {}", plan.legacy_config_count);
    println!(
        "  Legacy anchors converted as test history: {}",
        plan.legacy_anchor_count
    );
    println!(
        "  Treasury addresses repaired: {}",
        plan.repaired_config_count
    );
    println!(
        "  Anchor storage keys repaired: {}",
        plan.repaired_anchor_key_count
    );
    println!("  Backup: {}", backup_dir.display());
    println!("  Mode: {}", if args.apply { "APPLY" } else { "DRY RUN" });
}

fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_gets_deterministic_treasury_and_default_accounting() {
        let legacy = LegacyAppChainConfig {
            id: 7,
            name: "Legacy Chain".to_string(),
            owner: "operator".to_string(),
            genesis_root: [3u8; 32],
        };
        let raw = bincode::serialize(&legacy).unwrap();
        let (config, was_legacy) = decode_config(&raw).unwrap();

        assert!(was_legacy);
        assert_eq!(config.account_address, derive_appchain_treasury_address(7));
        assert_eq!(config.anchor_fee, DEFAULT_APPCHAIN_ANCHOR_FEE);
        assert_eq!(config.anchor_spend, 0);
        assert_eq!(config.anchor_count, 0);
    }

    #[test]
    fn legacy_anchor_becomes_zero_fee_test_history() {
        let legacy_config = LegacyAppChainConfig {
            id: 7,
            name: "Legacy Chain".to_string(),
            owner: "operator".to_string(),
            genesis_root: [0u8; 32],
        };
        let legacy_anchor = LegacyAnchoredState {
            chain_id: 7,
            state_root: "root".to_string(),
            proof: "legacy-proof".to_string(),
            timestamp: 42,
        };
        let plan = build_plan(
            &[RawRecord {
                key: 7u32.to_be_bytes().to_vec(),
                value: bincode::serialize(&legacy_config).unwrap(),
            }],
            &[RawRecord {
                key: [9u8; 32].to_vec(),
                value: bincode::serialize(&legacy_anchor).unwrap(),
            }],
        )
        .unwrap();

        assert_eq!(plan.anchors.len(), 1);
        assert_eq!(plan.anchors[0].anchor_number, 1);
        assert_eq!(plan.anchors[0].fee_charged, 0);
        assert!(plan.anchors[0].is_test);
        assert_eq!(plan.configs[0].anchor_count, 1);
    }
}
