use std::{fs, path::Path};

use axum_server::tls_rustls::RustlsConfig;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rcgen::generate_simple_self_signed;
use sha2::{Digest, Sha256};

pub struct TlsIdentity {
    pub config: RustlsConfig,
    pub fingerprint: String,
}

pub async fn load_or_create(state_dir: &Path, host: &str) -> Result<TlsIdentity, String> {
    fs::create_dir_all(state_dir)
        .map_err(|error| format!("cannot create {}: {error}", state_dir.display()))?;
    let certificate_path = state_dir.join("agent-cert.pem");
    let key_path = state_dir.join("agent-key.pem");
    if !certificate_path.exists() || !key_path.exists() {
        let certified = generate_simple_self_signed(vec![host.to_owned()])
            .map_err(|error| format!("cannot generate TLS identity: {error}"))?;
        fs::write(&certificate_path, certified.cert.pem())
            .map_err(|error| format!("cannot write {}: {error}", certificate_path.display()))?;
        fs::write(&key_path, certified.signing_key.serialize_pem())
            .map_err(|error| format!("cannot write {}: {error}", key_path.display()))?;
        #[cfg(unix)]
        restrict_private_key(&key_path)?;
    }

    let certificates = rustls_pemfile::certs(&mut std::io::BufReader::new(
        fs::File::open(&certificate_path)
            .map_err(|error| format!("cannot open {}: {error}", certificate_path.display()))?,
    ))
    .collect::<Result<Vec<_>, _>>()
    .map_err(|error| format!("invalid TLS certificate: {error}"))?;
    let certificate = certificates
        .first()
        .ok_or_else(|| "TLS certificate file is empty".to_owned())?;
    let fingerprint = URL_SAFE_NO_PAD.encode(Sha256::digest(certificate.as_ref()));
    let config = RustlsConfig::from_pem_file(&certificate_path, &key_path)
        .await
        .map_err(|error| format!("cannot load TLS identity: {error}"))?;
    Ok(TlsIdentity {
        config,
        fingerprint,
    })
}

#[cfg(unix)]
fn restrict_private_key(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot protect {}: {error}", path.display()))?;
    Ok(())
}
