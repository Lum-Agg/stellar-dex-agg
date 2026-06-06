//! Shared utilities for DEX adapters.

use {
    anyhow::{anyhow, Result},
    sha2::{Digest, Sha256},
};

/// Compute the Stellar Asset Contract (SAC) address for a classic asset.
/// SAC contract ID = SHA256(HashIdPreimage::ContractId(network_id, asset))
pub fn compute_sac_contract_id(asset: &str, network_passphrase: &str) -> Result<String> {
    if asset == "native" {
        // XLM SAC on mainnet (precomputed)
        return Ok("CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA".to_string());
    }

    let (code, issuer) = asset
        .split_once(':')
        .ok_or_else(|| anyhow!("Invalid asset format: {}", asset))?;

    let issuer_bytes = stellar_strkey::ed25519::PublicKey::from_string(issuer)
        .map_err(|e| anyhow!("Invalid issuer: {:?}", e))?
        .0;

    // Build XDR Asset
    let xdr_asset = build_xdr_asset(code, &issuer_bytes)?;

    // network_id = SHA256(network_passphrase)
    let network_id = Sha256::digest(network_passphrase.as_bytes());

    // Build HashIdPreimage::ContractId
    let mut preimage = Vec::new();
    // Discriminant for ContractId = 0 (EnvelopeType)
    preimage.extend_from_slice(&[0, 0, 0, 0]); // placeholder - actual XDR encoding needed
    preimage.extend_from_slice(&network_id);
    // ContractIdPreimage::Asset discriminant + asset XDR
    preimage.extend_from_slice(&xdr_asset);

    let hash = Sha256::digest(&preimage);
    Ok(format!("{}", stellar_strkey::Contract(hash.into())))
}

fn build_xdr_asset(code: &str, issuer: &[u8; 32]) -> Result<Vec<u8>> {
    let mut buf = Vec::new();

    if code.len() <= 4 {
        // CreditAlphanum4: type=1
        buf.extend_from_slice(&1u32.to_be_bytes());
        let mut code_bytes = [0u8; 4];
        code_bytes[..code.len()].copy_from_slice(code.as_bytes());
        buf.extend_from_slice(&code_bytes);
        // PublicKey type (ed25519 = 0)
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(issuer);
    } else {
        // CreditAlphanum12: type=2
        buf.extend_from_slice(&2u32.to_be_bytes());
        let mut code_bytes = [0u8; 12];
        code_bytes[..code.len().min(12)].copy_from_slice(&code.as_bytes()[..code.len().min(12)]);
        buf.extend_from_slice(&code_bytes);
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(issuer);
    }

    Ok(buf)
}

/// Check if a string looks like a Soroban contract address (C..., 56 chars).
pub fn is_contract_address(s: &str) -> bool {
    s.starts_with('C') && s.len() == 56
}

/// Parse asset string to determine if it's native, classic, or contract.
pub fn parse_asset_type(asset: &str) -> AssetType {
    if asset == "native" {
        AssetType::Native
    } else if asset.contains(':') {
        AssetType::Classic
    } else if is_contract_address(asset) {
        AssetType::Contract
    } else {
        AssetType::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetType {
    Native,
    Classic,
    Contract,
    Unknown,
}
