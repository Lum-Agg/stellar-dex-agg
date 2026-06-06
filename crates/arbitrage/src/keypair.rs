//! Executor signing key loading (single-key fallback).

use {
    anyhow::{anyhow, Result},
    soroban_client::keypair::{Keypair, KeypairBehavior},
};

/// Bot signing key.
#[derive(Clone)]
pub struct ExecutorKeypair(Keypair);

impl ExecutorKeypair {
    pub fn new(kp: Keypair) -> Self {
        Self(kp)
    }

    pub fn from_secret(secret: &str) -> Result<Self> {
        let kp = Keypair::from_secret(secret).map_err(|e| anyhow!("invalid secret key: {:?}", e))?;
        Ok(Self(kp))
    }

    pub fn public_key(&self) -> String {
        self.0.public_key()
    }

    pub fn inner(&self) -> &Keypair {
        &self.0
    }
}
