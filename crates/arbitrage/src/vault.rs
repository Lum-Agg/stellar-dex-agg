//! Vault helpers (Telegram balance reads; trade size uses `ARB_MAX_AMOUNT_IN`).

use {
    crate::context::ArbContext,
    anyhow::Result,
    dex_adapters::rpc::{scval_to_i128, SorobanRpc},
    stellar_strkey::Contract,
    stellar_xdr::curr as xdr,
};

const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";

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

/// Trade-size ceiling from config (`ARB_MAX_AMOUNT_IN`).
pub fn resolve_max_amount_in(ctx: &ArbContext, _base_token: &str) -> u128 {
    ctx.config.max_amount_in
}
