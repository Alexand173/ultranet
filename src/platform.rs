//! The operations whose spelling differs by operating system, kept in one place so that no call site
//! has to carry a `cfg` block of its own.
//!
//! This exists because of a specific failure: the Windows job could not compile a test module that
//! called `std::fs::Permissions::from_mode`, and the same call sat in the `backup` command of two
//! binaries, which `--all-targets` builds on every platform.

use std::io;
use std::path::Path;

/// Restrict a file to its owner, where the platform can express that.
///
/// Returns `Ok(true)` when mode 0600 was applied, and `Ok(false)` when the platform has no such
/// concept. Callers report which of the two happened rather than claiming a mode they did not set.
pub fn restrict_to_owner(path: &Path) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(true)
    }
    #[cfg(not(unix))]
    {
        // Windows expresses ownership through ACLs, which these services do not set. The backup is
        // still written; the message beside it says the mode was not applied here.
        let _ = path;
        Ok(false)
    }
}

/// Resolve when the platform asks this process to stop.
///
/// Unix has SIGTERM and the service units send it. Elsewhere there is no portable equivalent, so this
/// future never resolves and the `ctrl_c` arm of the caller's select is what stops the process — which
/// is why Ctrl-C is kept as its own arm rather than folded in here.
#[cfg(unix)]
pub async fn terminate() {
    use tokio::signal::unix::{signal, SignalKind};
    match signal(SignalKind::terminate()) {
        Ok(mut stream) => {
            stream.recv().await;
        }
        // A failed registration must not spin the select loop: wait forever instead.
        Err(_) => std::future::pending::<()>().await,
    }
}

#[cfg(not(unix))]
pub async fn terminate() {
    std::future::pending::<()>().await
}

/// The words that go next to a path whose permissions were just restricted, so the same sentence is
/// not rewritten in each command and does not overstate what happened.
pub fn describe_owner_only(applied: bool) -> &'static str {
    if applied {
        "mode 0600"
    } else {
        "mode not settable on this platform"
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn it_takes_the_group_and_other_bits_away() {
        use std::os::unix::fs::PermissionsExt;

        let path = std::env::temp_dir().join(format!("platform-perm-{}", std::process::id()));
        std::fs::write(&path, b"x").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(restrict_to_owner(&path).unwrap());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        std::fs::remove_file(&path).ok();
    }
}
