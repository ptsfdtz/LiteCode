use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

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
    failures: HashMap<String, FailureBucket>,
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
                failures: HashMap::new(),
            })),
            store_path: Arc::new(store_path),
        };
        service.persist()?;
        Ok(service)
    }

    pub fn invitation_uri(&self, endpoint: &str) -> String {
        let state = self.inner.lock().expect("auth mutex poisoned");
        format!(
            "litecode://pair?agent={}&endpoint={}&secret={}",
            state.store.agent_id,
            URL_SAFE_NO_PAD.encode(endpoint),
            state.invitation.secret
        )
    }

    pub fn pair(
        &self,
        source: &str,
        secret: &str,
        device_name: &str,
    ) -> Result<PairingResult, &'static str> {
        let mut state = self.inner.lock().expect("auth mutex poisoned");
        if rate_limited(&mut state.failures, source) {
            return Err("pairing_rate_limited");
        }
        let supplied_hash = hash(secret);
        let invitation_valid = !state.invitation.used
            && Instant::now() <= state.invitation.expires_at
            && bool::from(supplied_hash.ct_eq(&state.invitation.secret_hash));
        if !invitation_valid || device_name.trim().is_empty() || device_name.len() > 80 {
            record_failure(&mut state.failures, source);
            return Err("invalid_pairing_request");
        }

        let credential = random_token(32);
        let device_id = random_token(16);
        state.invitation.used = true;
        state.failures.remove(source);
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

    pub fn authenticate(&self, credential: &str) -> bool {
        let supplied = URL_SAFE_NO_PAD.encode(hash(credential));
        let state = self.inner.lock().expect("auth mutex poisoned");
        state.store.devices.iter().any(|device| {
            !device.revoked
                && bool::from(supplied.as_bytes().ct_eq(device.credential_hash.as_bytes()))
        })
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
        fs::rename(&temporary, self.store_path.as_ref())
            .map_err(|error| format!("cannot replace {}: {error}", self.store_path.display()))
    }
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
            .invitation_uri("ws://127.0.0.1")
            .split("secret=")
            .nth(1)
            .expect("secret in invitation")
            .to_owned();
        let paired = service.pair("local", &secret, "Test phone").expect("pairs");

        assert!(service.authenticate(&paired.credential));
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
}
