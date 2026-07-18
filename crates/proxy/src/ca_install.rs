//! Cross-platform installation of the MITM CA certificate into the system trust store.
//!
//! Supports three platforms:
//! - Windows: `certutil -addstore -user Root <cert>` (user-level store, no admin needed)
//! - macOS:   `security add-trusted-cert` into the login keychain (no sudo needed)
//! - Linux:   copy into `/usr/local/share/ca-certificates/` + `update-ca-certificates` (needs sudo)
//!
//! The CA certificate is identified by its Common Name (`AllTokens MITM CA`), so
//! uninstall/status operations match by CN and need no fingerprint/hash dependency.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The Common Name embedded in the CA certificate (see `ca.rs`).
pub const CA_COMMON_NAME: &str = "AllTokens MITM CA";

/// Target file name when installing into the Linux system trust anchor directory.
const LINUX_ANCHOR_NAME: &str = "alltokens-ca.crt";
const LINUX_ANCHOR_DIR: &str = "/usr/local/share/ca-certificates";

/// The trust store backend for the current platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustStore {
    Windows,
    MacOs,
    Linux,
}

impl TrustStore {
    /// Detect the trust store for the current compilation target.
    pub fn detect() -> TrustStore {
        if cfg!(target_os = "windows") {
            TrustStore::Windows
        } else if cfg!(target_os = "macos") {
            TrustStore::MacOs
        } else {
            TrustStore::Linux
        }
    }
}

/// Whether the CA is currently present in the system trust store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaInstallStatus {
    Installed,
    NotInstalled,
    Unknown,
}

// ---------------------------------------------------------------------------
// Pure argument builders (unit-testable, do not touch the system).
// ---------------------------------------------------------------------------

/// `certutil` args to add the CA to the current user's Root store.
fn windows_install_args(cert: &Path) -> Vec<String> {
    vec![
        "-addstore".to_string(),
        "-user".to_string(),
        "-f".to_string(),
        "Root".to_string(),
        cert.display().to_string(),
    ]
}

/// `certutil` args to remove the CA (matched by CN) from the user's Root store.
fn windows_uninstall_args() -> Vec<String> {
    vec![
        "-delstore".to_string(),
        "-user".to_string(),
        "Root".to_string(),
        CA_COMMON_NAME.to_string(),
    ]
}

/// `certutil` args to query the user's Root store for the CA (matched by CN).
fn windows_status_args() -> Vec<String> {
    vec![
        "-store".to_string(),
        "-user".to_string(),
        "Root".to_string(),
        CA_COMMON_NAME.to_string(),
    ]
}

/// The login keychain path for the current macOS user (`~/Library/Keychains/login.keychain-db`).
fn macos_login_keychain() -> Option<PathBuf> {
    dirs::home_dir().map(|h| {
        h.join("Library")
            .join("Keychains")
            .join("login.keychain-db")
    })
}

/// `security` args to add the CA as a trusted root in the login keychain.
fn macos_install_args(cert: &Path, keychain: &Path) -> Vec<String> {
    vec![
        "add-trusted-cert".to_string(),
        "-d".to_string(),
        "-r".to_string(),
        "trustRoot".to_string(),
        "-k".to_string(),
        keychain.display().to_string(),
        cert.display().to_string(),
    ]
}

/// `security` args to delete the CA (matched by CN) from keychains.
fn macos_uninstall_args() -> Vec<String> {
    vec![
        "delete-certificate".to_string(),
        "-c".to_string(),
        CA_COMMON_NAME.to_string(),
    ]
}

/// `security` args to find the CA (matched by CN) in keychains.
fn macos_status_args() -> Vec<String> {
    vec![
        "find-certificate".to_string(),
        "-c".to_string(),
        CA_COMMON_NAME.to_string(),
    ]
}

/// The destination path when installing into the Linux system trust anchors.
fn linux_anchor_path() -> PathBuf {
    Path::new(LINUX_ANCHOR_DIR).join(LINUX_ANCHOR_NAME)
}

// ---------------------------------------------------------------------------
// Execution entry points.
// ---------------------------------------------------------------------------

/// Install the CA certificate at `cert_path` into the current platform's trust store.
pub fn install(cert_path: &Path) -> Result<()> {
    if !cert_path.exists() {
        anyhow::bail!("CA certificate not found at {}", cert_path.display());
    }
    match TrustStore::detect() {
        TrustStore::Windows => {
            run_checked("certutil", &windows_install_args(cert_path), None)
                .context("install CA via certutil")?;
        },
        TrustStore::MacOs => {
            let keychain = macos_login_keychain()
                .context("resolve macOS login keychain path")?;
            run_checked("security", &macos_install_args(cert_path, &keychain), None)
                .context("install CA via security add-trusted-cert")?;
        },
        TrustStore::Linux => {
            let anchor = linux_anchor_path();
            std::fs::copy(cert_path, &anchor).with_context(|| {
                format!(
                    "copy CA to {} (may require sudo)",
                    anchor.display()
                )
            })?;
            run_checked("update-ca-certificates", &[], None)
                .context("run update-ca-certificates (may require sudo)")?;
        },
    }
    Ok(())
}

/// Uninstall the CA certificate (matched by CN) from the current platform's trust store.
pub fn uninstall(cert_path: &Path) -> Result<()> {
    match TrustStore::detect() {
        TrustStore::Windows => {
            run_checked("certutil", &windows_uninstall_args(), None)
                .context("uninstall CA via certutil")?;
        },
        TrustStore::MacOs => {
            run_checked("security", &macos_uninstall_args(), None)
                .context("uninstall CA via security delete-certificate")?;
        },
        TrustStore::Linux => {
            let anchor = linux_anchor_path();
            if anchor.exists() {
                std::fs::remove_file(&anchor).with_context(|| {
                    format!("remove {} (may require sudo)", anchor.display())
                })?;
            }
            run_checked("update-ca-certificates", &["--fresh".to_string()], None)
                .context("run update-ca-certificates --fresh (may require sudo)")?;
        },
    }
    // cert_path is unused on some platforms; keep signature symmetric with install().
    let _ = cert_path;
    Ok(())
}

/// Query whether the CA is currently installed in the trust store.
pub fn status(cert_path: &Path) -> Result<CaInstallStatus> {
    let _ = cert_path;
    match TrustStore::detect() {
        TrustStore::Windows => Ok(exit_to_status(run_status(
            "certutil",
            &windows_status_args(),
        ))),
        TrustStore::MacOs => Ok(exit_to_status(run_status(
            "security",
            &macos_status_args(),
        ))),
        TrustStore::Linux => {
            // On Linux the anchor file presence is the reliable signal.
            if linux_anchor_path().exists() {
                Ok(CaInstallStatus::Installed)
            } else {
                Ok(CaInstallStatus::NotInstalled)
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Command helpers.
// ---------------------------------------------------------------------------

/// Run a command and fail if it exits non-zero, surfacing stderr in the error.
fn run_checked(program: &str, args: &[String], cwd: Option<&Path>) -> Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let output = cmd
        .output()
        .with_context(|| format!("spawn `{program}` (is it installed and on PATH?)"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!(
            "`{program}` exited with {}: {}{}",
            output.status,
            stderr.trim(),
            stdout.trim()
        );
    }
    Ok(())
}

/// Run a status-probing command; returns `Some(success)` if it ran, `None` if it failed to spawn.
fn run_status(program: &str, args: &[String]) -> Option<bool> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .map(|o| o.status.success())
}

fn exit_to_status(ran: Option<bool>) -> CaInstallStatus {
    match ran {
        Some(true) => CaInstallStatus::Installed,
        Some(false) => CaInstallStatus::NotInstalled,
        None => CaInstallStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_matches_current_platform() {
        let detected = TrustStore::detect();
        #[cfg(target_os = "windows")]
        assert_eq!(detected, TrustStore::Windows);
        #[cfg(target_os = "macos")]
        assert_eq!(detected, TrustStore::MacOs);
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        assert_eq!(detected, TrustStore::Linux);
    }

    #[test]
    fn windows_install_args_shape() {
        let args = windows_install_args(Path::new("C:/ca/alltokens-ca.crt"));
        assert_eq!(args[0], "-addstore");
        assert_eq!(args[1], "-user");
        assert_eq!(args[2], "-f");
        assert_eq!(args[3], "Root");
        assert_eq!(args[4], "C:/ca/alltokens-ca.crt");
    }

    #[test]
    fn windows_uninstall_and_status_match_by_cn() {
        let un = windows_uninstall_args();
        assert_eq!(un[0], "-delstore");
        assert_eq!(un.last().unwrap(), CA_COMMON_NAME);

        let st = windows_status_args();
        assert_eq!(st[0], "-store");
        assert_eq!(st.last().unwrap(), CA_COMMON_NAME);
    }

    #[test]
    fn macos_args_shape() {
        let kc = Path::new("/Users/me/Library/Keychains/login.keychain-db");
        let install = macos_install_args(Path::new("/tmp/ca.crt"), kc);
        assert_eq!(install[0], "add-trusted-cert");
        assert!(install.iter().any(|a| a == "trustRoot"));
        assert_eq!(install.last().unwrap(), "/tmp/ca.crt");

        assert_eq!(macos_uninstall_args()[0], "delete-certificate");
        assert_eq!(macos_status_args()[0], "find-certificate");
        assert_eq!(macos_status_args().last().unwrap(), CA_COMMON_NAME);
    }

    #[test]
    fn linux_anchor_path_is_stable() {
        let p = linux_anchor_path();
        assert!(p.ends_with("alltokens-ca.crt"));
        assert!(p.starts_with(LINUX_ANCHOR_DIR));
    }

    #[test]
    fn exit_to_status_mapping() {
        assert_eq!(exit_to_status(Some(true)), CaInstallStatus::Installed);
        assert_eq!(exit_to_status(Some(false)), CaInstallStatus::NotInstalled);
        assert_eq!(exit_to_status(None), CaInstallStatus::Unknown);
    }
}
