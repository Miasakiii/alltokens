//! Certificate Authority for MITM TLS interception.
//!
//! Generates a self-signed root CA and issues per-host leaf certificates on the fly.
//! The root CA cert/key are stored as PEM files so users can install the CA in their
//! trust store for transparent interception.

use anyhow::{Context, Result};
use lru::LruCache;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType, DnValue, IsCa,
    KeyPair, KeyUsagePurpose, SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio_rustls::rustls::ServerConfig;

/// Maximum number of cached per-host TLS configs.
const CERT_CACHE_SIZE: usize = 256;

/// The Certificate Authority that signs leaf certificates for intercepted hosts.
pub struct CertificateAuthority {
    /// CA certificate in DER format (for building chains).
    ca_cert_der: CertificateDer<'static>,
    /// CA certificate (rcgen object, for signing leaf certs).
    ca_cert: Certificate,
    /// CA private key.
    ca_key_pair: KeyPair,
    /// Cache of generated ServerConfig per hostname.
    cache: Mutex<LruCache<String, std::sync::Arc<ServerConfig>>>,
}

impl CertificateAuthority {
    /// Load an existing CA from PEM files, or generate a new one if they don't exist.
    pub fn load_or_generate(ca_dir: &Path) -> Result<Self> {
        let cert_path = ca_dir.join("alltokens-ca.crt");
        let key_path = ca_dir.join("alltokens-ca.key");

        if cert_path.exists() && key_path.exists() {
            Self::load_from_pem(&cert_path, &key_path)
        } else {
            std::fs::create_dir_all(ca_dir)
                .context("create CA directory")?;
            Self::generate_and_save(&cert_path, &key_path)
        }
    }

    /// Generate a brand new CA and save to disk.
    fn generate_and_save(cert_path: &Path, key_path: &Path) -> Result<Self> {
        tracing::info!("Generating new MITM CA certificate at {}", cert_path.display());

        let key_pair = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
            .context("generate CA key pair")?;

        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, DnValue::Utf8String("AllTokens MITM CA".to_string()));
        dn.push(DnType::OrganizationName, DnValue::Utf8String("AllTokens".to_string()));
        params.distinguished_name = dn;

        // Valid for 10 years
        params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        params.not_after = rcgen::date_time_ymd(2034, 12, 31);

        let ca_cert = params.self_signed(&key_pair)
            .context("self-sign CA cert")?;

        // Save PEM files
        std::fs::write(cert_path, ca_cert.pem())
            .context("write CA cert PEM")?;
        std::fs::write(key_path, key_pair.serialize_pem())
            .context("write CA key PEM")?;

        tracing::info!("CA certificate saved to {}", cert_path.display());
        tracing::info!("Install this CA in your system trust store for MITM to work transparently");

        let ca_cert_der = CertificateDer::from(ca_cert.der().to_vec());

        Ok(Self {
            ca_cert_der,
            ca_cert,
            ca_key_pair: key_pair,
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(CERT_CACHE_SIZE).unwrap())),
        })
    }

    /// Load an existing CA from PEM files.
    fn load_from_pem(cert_path: &Path, key_path: &Path) -> Result<Self> {
        tracing::info!("Loading MITM CA from {}", cert_path.display());

        let cert_pem = std::fs::read_to_string(cert_path)
            .context("read CA cert PEM")?;
        let key_pem = std::fs::read_to_string(key_path)
            .context("read CA key PEM")?;

        // Parse certificate DER
        let mut cert_reader = std::io::Cursor::new(cert_pem.as_bytes());
        let cert_der = rustls_pemfile::certs(&mut cert_reader)
            .next()
            .ok_or_else(|| anyhow::anyhow!("no certificate found in PEM file"))?
            .context("parse CA cert PEM")?;

        // Parse private key
        let key_pair = KeyPair::from_pem(&key_pem)
            .context("parse CA private key from PEM")?;

        // Re-create the Certificate by self-signing with the same params and key.
        // This is needed because rcgen's `signed_by` requires a `&Certificate`.
        // The resulting cert object is used only for signing — the on-disk PEM
        // (which clients have installed) is what matters for trust validation.
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, DnValue::Utf8String("AllTokens MITM CA".to_string()));
        dn.push(DnType::OrganizationName, DnValue::Utf8String("AllTokens".to_string()));
        params.distinguished_name = dn;
        params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        params.not_after = rcgen::date_time_ymd(2034, 12, 31);

        let ca_cert = params.self_signed(&key_pair)
            .context("re-create CA cert for signing")?;

        Ok(Self {
            ca_cert_der: CertificateDer::from(cert_der.to_vec()),
            ca_cert,
            ca_key_pair: key_pair,
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(CERT_CACHE_SIZE).unwrap())),
        })
    }

    /// Get or create a TLS ServerConfig for the given hostname.
    /// The returned config contains a leaf cert signed by this CA, valid for the given host.
    pub fn get_server_config(&self, hostname: &str) -> Result<std::sync::Arc<ServerConfig>> {
        // Check cache first
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(config) = cache.get(hostname) {
                return Ok(config.clone());
            }
        }

        // Generate a new leaf certificate for this host
        let config = self.generate_leaf_config(hostname)?;
        let config = std::sync::Arc::new(config);

        // Store in cache
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(hostname.to_string(), config.clone());
        }

        Ok(config)
    }

    /// Generate a leaf certificate + TLS config for a specific hostname.
    fn generate_leaf_config(&self, hostname: &str) -> Result<ServerConfig> {
        let leaf_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
            .context("generate leaf key")?;

        let mut leaf_params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, DnValue::Utf8String(hostname.to_string()));
        leaf_params.distinguished_name = dn;

        // SAN: DNS name for the host
        leaf_params.subject_alt_names = vec![SanType::DnsName(hostname.try_into()
            .map_err(|e| anyhow::anyhow!("invalid DNS name '{hostname}': {e}"))?
        )];

        // Valid for 1 year
        leaf_params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        leaf_params.not_after = rcgen::date_time_ymd(2026, 12, 31);

        // Sign with CA
        let leaf_cert = leaf_params.signed_by(&leaf_key, &self.ca_cert, &self.ca_key_pair)
            .context("sign leaf cert with CA")?;

        // Build rustls ServerConfig
        let leaf_cert_der = CertificateDer::from(leaf_cert.der().to_vec());
        let leaf_key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![leaf_cert_der, self.ca_cert_der.clone()],
                leaf_key_der,
            )
            .context("build leaf TLS config")?;

        Ok(config)
    }

    /// Returns the path where the CA certificate is stored (for user installation).
    pub fn cert_path(ca_dir: &Path) -> PathBuf {
        ca_dir.join("alltokens-ca.crt")
    }

    /// Returns the CA certificate in PEM format (for display/export).
    pub fn ca_cert_pem(&self) -> String {
        // Re-encode DER to PEM
        let b64 = base64_encode(self.ca_cert_der.as_ref());
        format!("-----BEGIN CERTIFICATE-----\n{b64}\n-----END CERTIFICATE-----\n")
    }
}

/// Simple base64 encoder (64-char lines) for PEM output.
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let mut i = 0;
    let mut col = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() { data[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        let remaining = data.len() - i;

        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if remaining > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if remaining > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        col += 4;
        if col >= 64 {
            result.push('\n');
            col = 0;
        }
        i += 3;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_ca_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn generate_ca_creates_pem_files() {
        let dir = temp_ca_dir();
        let _ca = CertificateAuthority::load_or_generate(dir.path()).unwrap();

        let cert_path = dir.path().join("alltokens-ca.crt");
        let key_path = dir.path().join("alltokens-ca.key");
        assert!(cert_path.exists());
        assert!(key_path.exists());

        // PEM files should be valid
        let cert_pem = std::fs::read_to_string(&cert_path).unwrap();
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        let key_pem = std::fs::read_to_string(&key_path).unwrap();
        assert!(key_pem.contains("BEGIN"));
    }

    #[test]
    fn load_existing_ca_from_pem() {
        let dir = temp_ca_dir();
        // Generate first
        let _ca1 = CertificateAuthority::load_or_generate(dir.path()).unwrap();
        // Load again
        let ca2 = CertificateAuthority::load_or_generate(dir.path()).unwrap();
        // Should be able to generate leaf certs
        let config = ca2.get_server_config("api.openai.com").unwrap();
        assert!(std::sync::Arc::strong_count(&config) >= 1);
    }

    #[test]
    fn generate_leaf_cert_for_host() {
        let dir = temp_ca_dir();
        let ca = CertificateAuthority::load_or_generate(dir.path()).unwrap();

        let config = ca.get_server_config("api.openai.com").unwrap();
        // Second call should hit cache
        let config2 = ca.get_server_config("api.openai.com").unwrap();
        assert!(std::sync::Arc::ptr_eq(&config, &config2));

        // Different host should produce different config
        let config3 = ca.get_server_config("api.anthropic.com").unwrap();
        assert!(!std::sync::Arc::ptr_eq(&config, &config3));
    }
}
