use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::path::Path;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

const PAIRING_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_FAILURES: u8 = 5;
const FAILURE_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct AuthService {
    inner: Arc<Mutex<AuthState>>,
    store_path: Arc<PathBuf>,
}

struct AuthState {
    store: DeviceStore,
    invitation: PairingInvitation,
    pairing_failures: HashMap<String, FailureBucket>,
    authentication_failures: HashMap<String, FailureBucket>,
}

#[derive(Debug)]
struct PairingInvitation {
    secret_hash: [u8; 32],
    secret: String,
    expires_at: Instant,
    used: bool,
}

struct FailureBucket {
    count: u8,
    started_at: Instant,
}

#[derive(Deserialize, Serialize)]
struct DeviceStore {
    agent_id: String,
    devices: Vec<DeviceRecord>,
}

#[derive(Deserialize, Serialize)]
struct DeviceRecord {
    id: String,
    name: String,
    credential_hash: String,
    created_at_unix: u64,
    revoked: bool,
}

pub struct PairingResult {
    pub agent_id: String,
    pub device_id: String,
    pub credential: String,
}

pub struct DeviceSummary {
    pub id: String,
    pub name: String,
    pub created_at_unix: u64,
    pub revoked: bool,
}

impl AuthService {
    pub fn load(store_path: PathBuf) -> Result<Self, String> {
        let store = if store_path.exists() {
            let contents = fs::read_to_string(&store_path)
                .map_err(|error| format!("cannot read {}: {error}", store_path.display()))?;
            serde_json::from_str(&contents).map_err(|error| {
                format!("invalid device store {}: {error}", store_path.display())
            })?
        } else {
            DeviceStore {
                agent_id: random_token(16),
                devices: Vec::new(),
            }
        };
        let service = Self {
            inner: Arc::new(Mutex::new(AuthState {
                store,
                invitation: new_invitation(),
                pairing_failures: HashMap::new(),
                authentication_failures: HashMap::new(),
            })),
            store_path: Arc::new(store_path),
        };
        service.persist()?;
        Ok(service)
    }

    pub fn invitation_uri(&self, endpoint: &str, fingerprint: Option<&str>) -> String {
        let state = self.inner.lock().expect("auth mutex poisoned");
        let mut invitation = format!(
            "litecode://pair?agent={}&endpoint={}&secret={}",
            state.store.agent_id,
            URL_SAFE_NO_PAD.encode(endpoint),
            state.invitation.secret
        );
        if let Some(fingerprint) = fingerprint {
            invitation.push_str("&fingerprint=");
            invitation.push_str(fingerprint);
        }
        invitation
    }

    pub fn pair(
        &self,
        source: &str,
        secret: &str,
        device_name: &str,
    ) -> Result<PairingResult, &'static str> {
        let mut state = self.inner.lock().expect("auth mutex poisoned");
        if rate_limited(&mut state.pairing_failures, source) {
            return Err("pairing_rate_limited");
        }
        let supplied_hash = hash(secret);
        let invitation_valid = !state.invitation.used
            && Instant::now() <= state.invitation.expires_at
            && bool::from(supplied_hash.ct_eq(&state.invitation.secret_hash));
        if !invitation_valid || device_name.trim().is_empty() || device_name.len() > 80 {
            record_failure(&mut state.pairing_failures, source);
            return Err("invalid_pairing_request");
        }

        let credential = random_token(32);
        let device_id = random_token(16);
        state.invitation.used = true;
        state.pairing_failures.remove(source);
        state.store.devices.push(DeviceRecord {
            id: device_id.clone(),
            name: device_name.trim().to_owned(),
            credential_hash: URL_SAFE_NO_PAD.encode(hash(&credential)),
            created_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            revoked: false,
        });
        let result = PairingResult {
            agent_id: state.store.agent_id.clone(),
            device_id,
            credential,
        };
        drop(state);
        self.persist().map_err(|_| "device_store_error")?;
        Ok(result)
    }

    pub fn authenticate(&self, source: &str, credential: &str) -> Result<bool, &'static str> {
        let supplied = URL_SAFE_NO_PAD.encode(hash(credential));
        let mut state = self.inner.lock().expect("auth mutex poisoned");
        if rate_limited(&mut state.authentication_failures, source) {
            return Err("authentication_rate_limited");
        }
        let authenticated = state.store.devices.iter().any(|device| {
            !device.revoked
                && bool::from(supplied.as_bytes().ct_eq(device.credential_hash.as_bytes()))
        });
        if authenticated {
            state.authentication_failures.remove(source);
        } else {
            record_failure(&mut state.authentication_failures, source);
        }
        Ok(authenticated)
    }

    pub fn devices(&self) -> Vec<DeviceSummary> {
        let state = self.inner.lock().expect("auth mutex poisoned");
        state
            .store
            .devices
            .iter()
            .map(|device| DeviceSummary {
                id: device.id.clone(),
                name: device.name.clone(),
                created_at_unix: device.created_at_unix,
                revoked: device.revoked,
            })
            .collect()
    }

    pub fn revoke(&self, device_id: &str) -> Result<bool, String> {
        let mut state = self.inner.lock().expect("auth mutex poisoned");
        let Some(device) = state
            .store
            .devices
            .iter_mut()
            .find(|device| device.id == device_id)
        else {
            return Ok(false);
        };
        device.revoked = true;
        drop(state);
        self.persist()?;
        Ok(true)
    }

    fn persist(&self) -> Result<(), String> {
        let state = self.inner.lock().expect("auth mutex poisoned");
        let contents = serde_json::to_vec_pretty(&state.store)
            .map_err(|error| format!("cannot serialize device store: {error}"))?;
        if let Some(parent) = self.store_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        let temporary = self.store_path.with_extension("tmp");
        fs::write(&temporary, contents)
            .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
        #[cfg(unix)]
        restrict_file_permissions(&temporary)?;
        fs::rename(&temporary, self.store_path.as_ref())
            .map_err(|error| format!("cannot replace {}: {error}", self.store_path.display()))
    }
}

#[cfg(unix)]
fn restrict_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot protect {}: {error}", path.display()))?;
    Ok(())
}

fn new_invitation() -> PairingInvitation {
    let secret = random_token(24);
    PairingInvitation {
        secret_hash: hash(&secret),
        secret,
        expires_at: Instant::now() + PAIRING_TTL,
        used: false,
    }
}

fn random_token(bytes: usize) -> String {
    let mut value = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut value);
    URL_SAFE_NO_PAD.encode(value)
}

fn hash(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

fn rate_limited(failures: &mut HashMap<String, FailureBucket>, source: &str) -> bool {
    let Some(bucket) = failures.get_mut(source) else {
        return false;
    };
    if bucket.started_at.elapsed() > FAILURE_WINDOW {
        failures.remove(source);
        return false;
    }
    bucket.count >= MAX_FAILURES
}

fn record_failure(failures: &mut HashMap<String, FailureBucket>, source: &str) {
    let bucket = failures.entry(source.to_owned()).or_insert(FailureBucket {
        count: 0,
        started_at: Instant::now(),
    });
    bucket.count = bucket.count.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_store(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("litecode-{name}-{}.json", random_token(8)))
    }

    #[test]
    fn pairing_secret_is_single_use_and_credential_authenticates() {
        let path = temporary_store("pairing");
        let service = AuthService::load(path.clone()).expect("loads store");
        let secret = service
            .invitation_uri("ws://127.0.0.1", None)
            .split("secret=")
            .nth(1)
            .expect("secret in invitation")
            .to_owned();
        let paired = service.pair("local", &secret, "Test phone").expect("pairs");

        assert_eq!(service.authenticate("local", &paired.credential), Ok(true));
        assert_eq!(
            service.pair("local", &secret, "Second phone").err(),
            Some("invalid_pairing_request")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn pairing_failures_are_rate_limited() {
        let path = temporary_store("rate-limit");
        let service = AuthService::load(path.clone()).expect("loads store");
        for _ in 0..MAX_FAILURES {
            assert_eq!(
                service.pair("source", "wrong", "Phone").err(),
                Some("invalid_pairing_request")
            );
        }
        assert_eq!(
            service.pair("source", "wrong", "Phone").err(),
            Some("pairing_rate_limited")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn revoked_device_can_no_longer_authenticate() {
        let path = temporary_store("revoke");
        let service = AuthService::load(path.clone()).expect("loads store");
        let secret = service
            .invitation_uri("ws://127.0.0.1", None)
            .split("secret=")
            .nth(1)
            .expect("secret in invitation")
            .to_owned();
        let paired = service.pair("local", &secret, "Test phone").expect("pairs");

        assert!(service.revoke(&paired.device_id).expect("revokes"));
        assert_eq!(service.authenticate("local", &paired.credential), Ok(false));
        assert!(service.devices()[0].revoked);
        let _ = fs::remove_file(path);
    }
}
