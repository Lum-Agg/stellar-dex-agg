//! Pool of authorized bot accounts (stellar-arb `caller_indices` pattern).

use {
    crate::keypair::ExecutorKeypair,
    anyhow::{anyhow, Context, Result},
    soroban_client::keypair::{Keypair, KeypairBehavior},
    std::{
        collections::HashSet,
        sync::{
            atomic::{AtomicU64, AtomicUsize, Ordering},
            Arc,
        },
        time::{SystemTime, UNIX_EPOCH},
    },
    tokio::sync::Mutex,
    tracing::info,
};

/// After send, block caller reuse for ~one ledger (Stellar ~5s).
pub const DEFAULT_CALLER_COOLDOWN_MS: u64 = 5_000;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub struct CallerSlot {
    pub keypair: ExecutorKeypair,
    /// Short mutex: prepare + sign only (not poll).
    lock: Mutex<()>,
    /// Millis since epoch; caller skipped until this instant.
    busy_until_ms: AtomicU64,
}

pub struct CallerPool {
    slots: Vec<Arc<CallerSlot>>,
    /// Round-robin cursor so sequential acquires rotate callers.
    next: AtomicUsize,
}

impl CallerPool {
    pub fn from_config(
        mnemonic_path: Option<&str>,
        caller_indices: &[u32],
        secret_keys: &[String],
    ) -> Result<Option<Self>> {
        let mut keypairs = Vec::new();

        for secret in secret_keys {
            let trimmed = secret.trim();
            if trimmed.is_empty() {
                continue;
            }
            let kp = Keypair::from_secret(trimmed).map_err(|e| anyhow!("invalid ARB secret key: {:?}", e))?;
            keypairs.push(ExecutorKeypair::new(kp));
        }

        if let Some(path) = mnemonic_path {
            let phrase = std::fs::read_to_string(path).with_context(|| format!("read mnemonic file {}", path))?;
            let phrase = phrase.trim();
            for &idx in caller_indices {
                let kp = mnemonic_keypair(phrase, idx)?;
                keypairs.push(ExecutorKeypair::new(kp));
            }
        }

        let mut public_keys = HashSet::new();
        keypairs.retain(|keypair| public_keys.insert(keypair.public_key()));

        if keypairs.is_empty() {
            return Ok(None);
        }

        let slots: Vec<Arc<CallerSlot>> = keypairs
            .into_iter()
            .map(|kp| {
                Arc::new(CallerSlot {
                    keypair: kp,
                    lock: Mutex::new(()),
                    busy_until_ms: AtomicU64::new(0),
                })
            })
            .collect();

        info!(callers = slots.len(), "caller pool initialized");
        Ok(Some(Self {
            slots,
            next: AtomicUsize::new(0),
        }))
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn public_keys(&self) -> Vec<String> {
        self.slots.iter().map(|s| s.keypair.public_key()).collect()
    }

    /// Acquire a free caller (round-robin); returns None if all busy or in
    /// ledger cooldown.
    pub async fn try_acquire(&self) -> Option<CallerGuard<'_>> {
        let n = self.slots.len();
        if n == 0 {
            return None;
        }
        let now = now_ms();
        let start = self.next.fetch_add(1, Ordering::Relaxed) % n;
        for i in 0..n {
            let slot = &self.slots[(start + i) % n];
            if slot.busy_until_ms.load(Ordering::Relaxed) > now {
                continue;
            }
            if let Ok(guard) = slot.lock.try_lock() {
                return Some(CallerGuard { slot, _guard: guard });
            }
        }
        None
    }
}

impl CallerSlot {
    /// Reserve caller until ~one ledger after broadcast (seq safety without
    /// long mutex).
    pub fn mark_in_flight(&self, cooldown_ms: u64) {
        self.busy_until_ms
            .store(now_ms().saturating_add(cooldown_ms), Ordering::Relaxed);
    }
}

pub struct CallerGuard<'a> {
    slot: &'a Arc<CallerSlot>,
    _guard: tokio::sync::MutexGuard<'a, ()>,
}

impl CallerGuard<'_> {
    pub fn keypair(&self) -> &ExecutorKeypair {
        &self.slot.keypair
    }

    pub fn public_key(&self) -> String {
        self.slot.keypair.public_key()
    }

    pub fn mark_in_flight(&self, cooldown_ms: u64) {
        self.slot.mark_in_flight(cooldown_ms);
    }
}

fn mnemonic_keypair(phrase: &str, index: u32) -> Result<Keypair> {
    use bip39::{Language, Mnemonic};

    let mnemonic = Mnemonic::parse_in(Language::English, phrase).map_err(|e| anyhow!("invalid mnemonic: {:?}", e))?;
    let seed = mnemonic.to_seed("");
    let path = [0x8000_0000 + 44, 0x8000_0000 + 148, 0x8000_0000 + index];
    let derived = slip10_ed25519::derive_ed25519_private_key(&seed, &path);
    Keypair::from_raw_ed25519_seed(&derived).map_err(|e| anyhow!("derive keypair: {:?}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_robin_rotates_across_free_callers() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let dir = std::env::temp_dir().join(format!("arb-caller-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("mnemonic.txt");
        std::fs::write(&path, phrase).unwrap();

        let pool = CallerPool::from_config(Some(path.to_str().unwrap()), &[1, 2, 3], &[])
            .unwrap()
            .unwrap();
        assert_eq!(pool.len(), 3);

        let a = pool.try_acquire().await.unwrap();
        let b = pool.try_acquire().await.unwrap();
        let c = pool.try_acquire().await.unwrap();
        assert_ne!(a.public_key(), b.public_key());
        assert_ne!(b.public_key(), c.public_key());
        assert_ne!(a.public_key(), c.public_key());
        assert!(pool.try_acquire().await.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn duplicate_caller_indices_share_one_slot() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let dir = std::env::temp_dir().join(format!("arb-caller-dedup-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("mnemonic.txt");
        std::fs::write(&path, phrase).unwrap();

        let pool = CallerPool::from_config(Some(path.to_str().unwrap()), &[1, 1], &[])
            .unwrap()
            .unwrap();
        assert_eq!(pool.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
