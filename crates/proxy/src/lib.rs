//! AllTokens 透明代理引擎
//! Phase 3: MITM HTTPS 代理，拦截已知 API endpoint

pub mod ca;
pub mod ca_install;
pub mod intercept;
pub mod mitm;
pub mod server;

use alltokens_core::pricing::PricingEngine;
use alltokens_core::storage::Storage;
use std::sync::Arc;

pub use ca::CertificateAuthority;
pub use ca_install::{install, status, uninstall, CaInstallStatus, TrustStore};
pub use intercept::{extract_usage_from_body, extract_usage_from_sse, should_intercept_host};
pub use server::{handle_connection, run_proxy, ProxyHandle};

pub struct ProxyConfig {
    pub listen_addr: std::net::SocketAddr,
    pub ca_cert_path: Option<std::path::PathBuf>,
    /// Enable MITM TLS interception (requires CA).
    /// When true and ca_cert_path is set, CONNECT tunnels to known API hosts
    /// will be intercepted for usage extraction.
    pub mitm_enabled: bool,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:7890".parse().unwrap(),
            ca_cert_path: None,
            mitm_enabled: false,
        }
    }
}

impl ProxyConfig {
    /// Resolve the CA directory path (defaults to ~/.alltokens/ca/ if not specified).
    pub fn ca_dir(&self) -> std::path::PathBuf {
        if let Some(ref path) = self.ca_cert_path {
            path.clone()
        } else {
            dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("alltokens")
                .join("ca")
        }
    }
}

/// 启动代理服务器 (forward-only when no CA, MITM when CA is configured)
pub async fn start_proxy(config: ProxyConfig) -> anyhow::Result<()> {
    let ca = if config.mitm_enabled {
        let ca_dir = config.ca_dir();
        let ca = CertificateAuthority::load_or_generate(&ca_dir)?;
        Some(Arc::new(ca))
    } else {
        None
    };
    run_proxy(config.listen_addr, None, ca).await
}

/// 启动代理并将拦截到的 usage 写入 SQLite（含定价计算）
pub async fn start_proxy_with_storage(
    config: ProxyConfig,
    storage: Storage,
    pricing: PricingEngine,
) -> anyhow::Result<()> {
    let storage = Arc::new(storage);
    let pricing = Arc::new(pricing);
    let on_usage: server::UsageCallback = Arc::new(move |mut record| {
        pricing.calculate_cost(&mut record);
        if let Err(e) = storage.insert_record(&record) {
            tracing::warn!("Failed to persist proxy record: {e}");
        } else {
            tracing::debug!(
                "Persisted proxy record: {} {} ({} tokens)",
                record.provider,
                record.model,
                record.total_tokens
            );
        }
    });

    let ca = if config.mitm_enabled {
        let ca_dir = config.ca_dir();
        let ca = CertificateAuthority::load_or_generate(&ca_dir)?;
        Some(Arc::new(ca))
    } else {
        None
    };

    run_proxy(config.listen_addr, Some(on_usage), ca).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_localhost_7890() {
        let cfg = ProxyConfig::default();
        assert_eq!(cfg.listen_addr.port(), 7890);
        assert!(!cfg.mitm_enabled);
    }

    #[test]
    fn config_ca_dir_uses_default_when_no_path() {
        let cfg = ProxyConfig::default();
        let ca_dir = cfg.ca_dir();
        assert!(ca_dir.to_string_lossy().contains("alltokens"));
    }
}
