//! Shared utilities for DEX adapters.

use {
    anyhow::{anyhow, Result},
    sha2::{Digest, Sha256},
    stellar_xdr::curr::{
        Asset, AssetCode12, AssetCode4, ContractIdPreimage, Hash, HashIdPreimage, HashIdPreimageContractId, Limits,
        PublicKey, Uint256, WriteXdr,
    },
};

/// Compute the Stellar Asset Contract (SAC) address for a classic asset.
///
/// SAC contract IDs include the network ID, so the same asset has different
/// addresses on mainnet and testnet.
pub fn compute_sac_contract_id(asset: &str, network_passphrase: &str) -> Result<String> {
    let asset = if asset == "native" {
        Asset::Native
    } else {
        let (code, issuer) = asset
            .split_once(':')
            .ok_or_else(|| anyhow!("Invalid asset format: {asset}"))?;
        classic_asset(code, issuer)?
    };

    let preimage = HashIdPreimage::ContractId(HashIdPreimageContractId {
        network_id: Hash(Sha256::digest(network_passphrase.as_bytes()).into()),
        contract_id_preimage: ContractIdPreimage::Asset(asset),
    })
    .to_xdr(Limits::none())
    .map_err(|e| anyhow!("encode SAC contract preimage: {e:?}"))?;

    Ok(stellar_strkey::Contract(Sha256::digest(preimage).into())
        .to_string()
        .to_string())
}

fn classic_asset(code: &str, issuer: &str) -> Result<Asset> {
    let issuer =
        stellar_strkey::ed25519::PublicKey::from_string(issuer).map_err(|e| anyhow!("Invalid issuer: {:?}", e))?;
    let issuer = stellar_xdr::curr::AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(issuer.0)));
    let code_bytes = code.as_bytes();

    match code_bytes.len() {
        1..=4 => {
            let mut padded = [0u8; 4];
            padded[..code_bytes.len()].copy_from_slice(code_bytes);
            Ok(Asset::CreditAlphanum4(stellar_xdr::curr::AlphaNum4 {
                asset_code: AssetCode4(padded),
                issuer,
            }))
        }
        5..=12 => {
            let mut padded = [0u8; 12];
            padded[..code_bytes.len()].copy_from_slice(code_bytes);
            Ok(Asset::CreditAlphanum12(stellar_xdr::curr::AlphaNum12 {
                asset_code: AssetCode12(padded),
                issuer,
            }))
        }
        _ => Err(anyhow!("Classic asset code must be 1-12 bytes")),
    }
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

#[cfg(test)]
mod tests {
    use super::compute_sac_contract_id;

    const MAINNET: &str = "Public Global Stellar Network ; September 2015";
    const TESTNET: &str = "Test SDF Network ; September 2015";
    const USDC: &str = "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

    #[test]
    fn derives_mainnet_native_sac() {
        assert_eq!(
            compute_sac_contract_id("native", MAINNET).unwrap(),
            "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
        );
    }

    #[test]
    fn derives_mainnet_usdc_sac() {
        assert_eq!(
            compute_sac_contract_id(USDC, MAINNET).unwrap(),
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"
        );
    }

    #[test]
    fn sac_addresses_are_network_specific() {
        assert_ne!(
            compute_sac_contract_id("native", MAINNET).unwrap(),
            compute_sac_contract_id("native", TESTNET).unwrap()
        );
        assert_ne!(
            compute_sac_contract_id(USDC, MAINNET).unwrap(),
            compute_sac_contract_id(USDC, TESTNET).unwrap()
        );
    }

    #[test]
    fn rejects_invalid_classic_assets() {
        assert!(compute_sac_contract_id("", MAINNET).is_err());
        assert!(compute_sac_contract_id(
            "TOO_LONG_ASSET:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
            MAINNET
        )
        .is_err());
        assert!(compute_sac_contract_id("USDC:not-an-account", MAINNET).is_err());
    }
}
