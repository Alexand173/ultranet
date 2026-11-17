//! Anchoring a day, by running the existing `appchain-cli`.
//!
//! The service never holds the owner key in memory and never re-implements signing: it runs
//! `appchain-cli anchor`, which reads the encrypted key file with a passphrase from a file, signs the
//! version-5 envelope and submits it. That keeps one signing implementation for every chain, and means
//! a bug in this service cannot change what an anchor transaction is.
//!
//! The passphrase is passed as a **file path**, never as an argument, so it cannot appear in a process list.

use async_trait::async_trait;
use std::{path::PathBuf, time::Duration};
use thiserror::Error;
use tokio::process::Command;

#[derive(Debug, Error)]
pub enum AnchorError {
    #[error("the anchor command could not be run: {0}")]
    Spawn(String),
    #[error("the anchor command failed: {0}")]
    Failed(String),
    #[error("the anchor command timed out")]
    TimedOut,
    #[error("the anchor command's output was not understood")]
    Unparsed,
}

#[async_trait]
pub trait Anchorer: Send + Sync {
    /// Submit `root` as the next anchor of `chain_id`. Returns the transaction hash.
    ///
    /// `Ok` means the node accepted the transaction as pending, not that a block holds it yet. The
    /// caller confirms from the ledger, and treats any error as *ambiguous*: the transaction may or
    /// may not have been accepted, so the next attempt looks at the ledger before it submits again.
    async fn anchor(&self, chain_id: u32, root: &[u8; 32]) -> Result<String, AnchorError>;
}

/// Anchoring by asking the Records Service, which holds the owner key. This is what the Games Reward
/// Service uses: it never holds a signing key for any chain. The request is signed with the shared peer
/// secret (see `peer.rs`) and sent over loopback.
///
/// Any error is *ambiguous*, exactly as for the CLI: the Records Service may have submitted the anchor
/// before the connection failed. The caller looks at the ledger before it asks again, and so does the
/// Records Service, so an ambiguous failure never becomes a second anchor.
pub struct RemoteAnchorer {
    pub records_url: String,
    pub secret: Vec<u8>,
    pub client: reqwest::Client,
    pub clock: std::sync::Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl RemoteAnchorer {
    pub fn new(records_url: &str, secret: Vec<u8>, timeout: Duration) -> Result<RemoteAnchorer, String> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(RemoteAnchorer {
            records_url: records_url.trim_end_matches('/').to_string(),
            secret,
            client,
            clock: std::sync::Arc::new(crate::auth::now_seconds),
        })
    }

    /// Sign with another clock (tests give both services the same one).
    pub fn with_clock(mut self, clock: std::sync::Arc<dyn Fn() -> u64 + Send + Sync>) -> RemoteAnchorer {
        self.clock = clock;
        self
    }
}

#[async_trait]
impl Anchorer for RemoteAnchorer {
    async fn anchor(&self, chain_id: u32, root: &[u8; 32]) -> Result<String, AnchorError> {
        const PATH: &str = "/internal/v1/anchor";
        let body = serde_json::json!({ "chain_id": chain_id, "root": hex::encode(root) }).to_string();
        let signed = super::peer::sign_request(&self.secret, "games", "POST", PATH, body.as_bytes(), (self.clock)());
        let response = self
            .client
            .post(format!("{}{PATH}", self.records_url))
            .header(super::peer::HEADER_PEER, &signed.peer)
            .header(super::peer::HEADER_TIMESTAMP, signed.timestamp.to_string())
            .header(super::peer::HEADER_NONCE, &signed.nonce)
            .header(super::peer::HEADER_SIGNATURE, &signed.signature)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    AnchorError::TimedOut
                } else {
                    AnchorError::Spawn(format!("the Records Service could not be reached: {error}"))
                }
            })?;
        let status = response.status();
        let reply: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
        if !status.is_success() {
            let message = reply["message"].as_str().unwrap_or("no message");
            let code = reply["code"].as_str().unwrap_or("error");
            return Err(AnchorError::Failed(format!("{code} ({status}): {message}").chars().take(200).collect()));
        }
        let hash = reply["tx_hash"].as_str().ok_or(AnchorError::Unparsed)?;
        if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            Ok(hash.to_ascii_lowercase())
        } else {
            Err(AnchorError::Unparsed)
        }
    }
}

pub struct CliAnchorer {
    pub cli_path: PathBuf,
    pub key_file: PathBuf,
    pub password_file: PathBuf,
    pub node_url: String,
    pub timeout: Duration,
}

/// `accepted, pending a block: <64 hex>` is the line the CLI prints on success.
pub fn parse_cli_output(stdout: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let hash = line
            .trim()
            .strip_prefix("accepted, pending a block:")?
            .trim();
        (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

#[async_trait]
impl Anchorer for CliAnchorer {
    async fn anchor(&self, chain_id: u32, root: &[u8; 32]) -> Result<String, AnchorError> {
        let mut command = Command::new(&self.cli_path);
        command
            .arg("anchor")
            .arg("--key-file")
            .arg(&self.key_file)
            .arg("--password-file")
            .arg(&self.password_file)
            .arg("--chain-id")
            .arg(chain_id.to_string())
            .arg("--hash")
            .arg(hex::encode(root))
            .arg("--node")
            .arg(&self.node_url)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        let output = tokio::time::timeout(self.timeout, command.output())
            .await
            .map_err(|_| AnchorError::TimedOut)?
            .map_err(|error| AnchorError::Spawn(error.to_string()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let first = stderr
                .lines()
                .next()
                .unwrap_or("no message")
                .chars()
                .take(200)
                .collect::<String>();
            return Err(AnchorError::Failed(first));
        }
        parse_cli_output(&String::from_utf8_lossy(&output.stdout)).ok_or(AnchorError::Unparsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn the_cli_success_line_is_parsed_and_nothing_else_is() {
        let hash = "ab".repeat(32);
        let out = format!(
            "accepted, pending a block: {hash}\nnonce 4, fee 1000 base units, signed by x\n"
        );
        assert_eq!(parse_cli_output(&out), Some(hash.clone()));
        assert_eq!(
            parse_cli_output(&format!("ACCEPTED, pending a block: {hash}")),
            None
        );
        assert_eq!(parse_cli_output("accepted, pending a block: nothex"), None);
        assert_eq!(parse_cli_output("accepted, pending a block: abcd"), None);
        assert_eq!(parse_cli_output(""), None);
    }

    fn script(name: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("games-anchor-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fake-cli.sh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn anchorer(cli: PathBuf, timeout: Duration) -> CliAnchorer {
        CliAnchorer {
            cli_path: cli,
            key_file: "/keys/owner.enc".into(),
            password_file: "/keys/owner.pw".into(),
            node_url: "https://node.example".into(),
            timeout,
        }
    }

    #[tokio::test]
    async fn the_command_receives_exactly_the_documented_arguments() {
        // The fake CLI echoes its arguments into a file, then prints the success line.
        let out = std::env::temp_dir().join(format!("games-anchor-args-{}", std::process::id()));
        let hash = "cd".repeat(32);
        let cli = script(
            "args",
            &format!(
                "echo \"$@\" > {}\necho \"accepted, pending a block: {hash}\"",
                out.display()
            ),
        );
        let result = anchorer(cli, Duration::from_secs(10))
            .anchor(1, &[0x11; 32])
            .await
            .unwrap();
        assert_eq!(result, hash);
        let args = std::fs::read_to_string(&out).unwrap();
        assert_eq!(
            args.trim(),
            format!(
                "anchor --key-file /keys/owner.enc --password-file /keys/owner.pw --chain-id 1 --hash {} --node https://node.example",
                "11".repeat(32)
            )
        );
        // The passphrase is only ever a path: no secret can be in argv.
        assert!(
            !args.contains("--password "),
            "no inline passphrase option exists"
        );
    }

    #[tokio::test]
    async fn a_failing_command_is_an_error_with_its_first_line() {
        let cli = script(
            "fail",
            "echo 'wrong passphrase for this key file' >&2\necho second line >&2\nexit 1",
        );
        let error = anchorer(cli, Duration::from_secs(10))
            .anchor(1, &[0; 32])
            .await
            .unwrap_err();
        match error {
            AnchorError::Failed(message) => {
                assert_eq!(message, "wrong passphrase for this key file")
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn success_without_the_expected_line_is_not_trusted() {
        let cli = script("garbled", "echo 'done'");
        assert!(matches!(
            anchorer(cli, Duration::from_secs(10))
                .anchor(1, &[0; 32])
                .await,
            Err(AnchorError::Unparsed)
        ));
    }

    #[tokio::test]
    async fn a_hung_command_times_out_and_is_killed() {
        let cli = script("hang", "sleep 30");
        let started = std::time::Instant::now();
        let error = anchorer(cli, Duration::from_millis(300))
            .anchor(1, &[0; 32])
            .await
            .unwrap_err();
        assert!(matches!(error, AnchorError::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_missing_binary_is_a_spawn_error() {
        let error = anchorer("/nonexistent/appchain-cli".into(), Duration::from_secs(5))
            .anchor(1, &[0; 32])
            .await
            .unwrap_err();
        assert!(matches!(error, AnchorError::Spawn(_)));
    }
}
