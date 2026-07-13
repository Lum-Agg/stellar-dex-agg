//! Vault float checks — cap trade size to available base token balance.

use {
    crate::context::ArbContext,
    anyhow::Result,
    dex_adapters::rpc::{scval_to_i128, SorobanRpc},
    stellar_strkey::Contract,
    stellar_xdr::curr as xdr,
    tracing::{debug, warn},
};

const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";
/// Keep a small XLM buffer so rounding / fees do not fail vault.transfer.
const BALANCE_BUFFER_STROOPS: u128 = 10_000_000; // 1 XLM

fn contract_holder_scval(contract_id: &str) -> Result<xdr::ScVal> {
    let hash = Contract::from_string(contract_id)
        .map_err(|e| anyhow::anyhow!("invalid contract id {}: {:?}", contract_id, e))?
        .0;
    Ok(xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(
        xdr::Hash(hash),
    ))))
}

/// SAC `balance` for `holder` (vault contract address).
pub async fn fetch_token_balance_stroops(rpc_url: &str, token: &str, holder_contract: &str) -> Result<u128> {
    let rpc = SorobanRpc::new(rpc_url, MAINNET_PASSPHRASE);
    let holder = contract_holder_scval(holder_contract)?;
    let val = rpc.simulate_call(token, "balance", vec![holder]).await?;
    let balance = scval_to_i128(&val).map_err(|e| anyhow::anyhow!("parse balance: {}", e))?;
    u128::try_from(balance).map_err(|_| anyhow::anyhow!("negative token balance"))
}

/// Cap configured `max_amount_in` by vault base float when vault mode is
/// enabled.
pub fn cap_max_amount_in(config_max: u128, vault_balance: Option<u128>) -> u128 {
    let Some(balance) = vault_balance else {
        return config_max;
    };
    let available = balance.saturating_sub(BALANCE_BUFFER_STROOPS);
    config_max.min(available)
}

pub async fn resolve_max_amount_in(ctx: &ArbContext, base_token: &str) -> u128 {
    let capped = cap_max_amount_in(ctx.config.max_amount_in, ctx.vault_base_balance);
    if capped < ctx.config.min_amount_in {
        warn!(
            base = %base_token,
            vault_balance = ?ctx.vault_base_balance,
            capped,
            min_amount_in = ctx.config.min_amount_in,
            "vault base balance below min trade size"
        );
        return 0;
    }
    if let Some(balance) = ctx.vault_base_balance {
        if capped < ctx.config.max_amount_in {
            debug!(
                base = %base_token,
                vault_balance = balance,
                capped,
                config_max = ctx.config.max_amount_in,
                "capped max_amount_in to vault balance"
            );
        }
    }
    capped
}

#[cfg(test)]
mod tests {
    use super::cap_max_amount_in;

    #[test]
    fn caps_to_vault_balance_with_buffer() {
        // vault 1800 XLM, config max 18k XLM -> capped to 1790 XLM
        assert_eq!(cap_max_amount_in(180_000_000_000, Some(18_000_000_000)), 17_990_000_000);
    }

    #[test]
    fn no_vault_uses_config_max() {
        assert_eq!(cap_max_amount_in(100_000_000, None), 100_000_000);
    }
}
