#[cfg(unix)]
use clap::Parser;
#[cfg(unix)]
use std::os::unix::{fs::FileTypeExt, io::FromRawFd, net::UnixListener as StdUnixListener};
#[cfg(unix)]
use std::{env, path::PathBuf, sync::Arc};
#[cfg(unix)]
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};
#[cfg(unix)]
use UltraNet::approval_signer::{
    ApprovalSigner, FileApprovalSigner, SignerRequest, SignerResponse,
};

#[cfg(unix)]
#[derive(Debug, Parser)]
#[command(
    name = "ultranet-approval-signer",
    version,
    about = "Isolated local signer for UltraNet version-3 validator approvals"
)]
struct Cli {
    /// Unix socket used only by the private approval gateway.
    #[arg(long, env = "ULTRANET_APPROVAL_SIGNER_SOCKET")]
    socket: PathBuf,

    /// The one-record or three-record private owner key file. Keep it offline/private.
    #[arg(long, env = "ULTRANET_SIGNER_KEY_FILE")]
    keys: PathBuf,

    /// Configured Sovereign owner index represented by this signer process.
    #[arg(long, env = "ULTRANET_SIGNER_OWNER_INDEX")]
    owner_index: usize,

    /// Zero-based record index in the local private key file. Use 0 when the file contains one owner.
    #[arg(long, env = "ULTRANET_SIGNER_KEY_INDEX", default_value_t = 0)]
    key_index: usize,

    /// Non-secret signer identity bound by the gateway configuration.
    #[arg(long, env = "ULTRANET_SIGNER_ID")]
    signer_id: String,

    /// Permit an unattended signer only when the explicit safety environment
    /// acknowledgement is present. Production should use local confirmation or HSM presence.
    #[arg(long)]
    unattended: bool,
}

#[cfg(unix)]
#[tokio::main]
async fn main() -> Result<(), String> {
    let cli = Cli::parse();
    let require_confirmation = !cli.unattended;
    if cli.unattended
        && std::env::var("ULTRANET_APPROVAL_SIGNER_ALLOW_UNATTENDED")
            .ok()
            .as_deref()
            != Some("I_UNDERSTAND")
    {
        return Err(
            "--unattended requires ULTRANET_APPROVAL_SIGNER_ALLOW_UNATTENDED=I_UNDERSTAND".into(),
        );
    }

    let signer = Arc::new(FileApprovalSigner::from_key_file(
        &cli.keys,
        cli.key_index,
        cli.owner_index,
        cli.signer_id,
        require_confirmation,
    )?);
    let listener = match socket_activation_listener(&cli.socket)? {
        Some(listener) => listener,
        None => {
            if cli.socket.exists() {
                let metadata = std::fs::symlink_metadata(&cli.socket)
                    .map_err(|error| format!("cannot inspect existing signer socket: {error}"))?;
                if !metadata.file_type().is_socket() {
                    return Err(format!(
                        "refusing to replace non-socket path {}",
                        cli.socket.display()
                    ));
                }
                std::fs::remove_file(&cli.socket)
                    .map_err(|error| format!("cannot remove stale signer socket: {error}"))?;
            }

            let listener = UnixListener::bind(&cli.socket).map_err(|error| {
                format!(
                    "cannot bind signer socket {}: {error}",
                    cli.socket.display()
                )
            })?;
            set_socket_permissions(&cli.socket)?;
            listener
        }
    };
    eprintln!(
        "UltraNet approval signer listening on {}",
        cli.socket.display()
    );

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|error| format!("signer socket accept failed: {error}"))?;
        let signer = signer.clone();
        if let Err(error) = handle_connection(stream, signer).await {
            eprintln!("approval signer request rejected: {error}");
        }
    }
}

#[cfg(unix)]
async fn handle_connection(
    stream: tokio::net::UnixStream,
    signer: Arc<FileApprovalSigner>,
) -> Result<(), String> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut frame = Vec::new();
    reader
        .read_until(b'\n', &mut frame)
        .await
        .map_err(|error| format!("cannot read signer request: {error}"))?;
    if frame.is_empty() || frame.len() > 128 * 1024 {
        return Err("signer request frame is empty or too large".into());
    }
    let request = serde_json::from_slice::<SignerRequest>(&frame)
        .map_err(|error| format!("invalid signer request: {error}"))?;
    let response: Result<SignerResponse, String> = signer.sign(request).await;
    let encoded = match response {
        Ok(response) => serde_json::to_vec(&response)
            .map_err(|error| format!("cannot encode signer response: {error}"))?,
        Err(error) => serde_json::to_vec(&serde_json::json!({
            "error": error,
        }))
        .map_err(|encode_error| format!("cannot encode signer error: {encode_error}"))?,
    };
    write_half
        .write_all(&encoded)
        .await
        .map_err(|error| format!("cannot write signer response: {error}"))?;
    write_half
        .write_all(b"\n")
        .await
        .map_err(|error| format!("cannot terminate signer response: {error}"))?;
    write_half
        .shutdown()
        .await
        .map_err(|error| format!("cannot close signer response: {error}"))?;
    Ok(())
}

#[cfg(unix)]
fn socket_activation_listener(path: &PathBuf) -> Result<Option<UnixListener>, String> {
    let listen_pid = env::var("LISTEN_PID").ok();
    let listen_fds = env::var("LISTEN_FDS").ok();
    if listen_pid.is_none() && listen_fds.is_none() {
        return Ok(None);
    }

    let listen_pid = listen_pid
        .ok_or_else(|| "LISTEN_PID is missing for socket activation".to_string())?
        .parse::<u32>()
        .map_err(|_| "LISTEN_PID is invalid for socket activation".to_string())?;
    let listen_fds = listen_fds
        .ok_or_else(|| "LISTEN_FDS is missing for socket activation".to_string())?
        .parse::<usize>()
        .map_err(|_| "LISTEN_FDS is invalid for socket activation".to_string())?;
    if listen_pid != std::process::id() {
        return Err("LISTEN_PID does not belong to this signer process".into());
    }
    if listen_fds != 1 {
        return Err(format!(
            "approval signer requires exactly one activated socket; received {listen_fds}"
        ));
    }

    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect activated signer socket: {error}"))?;
    if !metadata.file_type().is_socket() {
        return Err(format!(
            "activated signer path is not a socket: {}",
            path.display()
        ));
    }

    // systemd reserves fd 3 for the first socket in LISTEN_FDS. Ownership of
    // the descriptor transfers to this listener, so it closes on shutdown.
    let listener = unsafe { StdUnixListener::from_raw_fd(3) };
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("cannot configure activated signer socket: {error}"))?;
    UnixListener::from_std(listener)
        .map(Some)
        .map_err(|error| format!("cannot adopt activated signer socket: {error}"))
}

#[cfg(unix)]
fn set_socket_permissions(path: &PathBuf) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))
        .map_err(|error| format!("cannot restrict signer socket permissions: {error}"))?;
    Ok(())
}

#[cfg(not(unix))]
fn main() {
    eprintln!("UltraNet approval signer requires a Unix-domain socket host.");
    std::process::exit(1);
}
