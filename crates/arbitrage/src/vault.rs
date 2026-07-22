//! Vault helpers (balance reads for size caps + Telegram).

use {
    crate::context::ArbContext,
    anyhow::Result,
    dex_adapters::rpc::{scval_to_i128, SorobanRpc},
    stellar_strkey::Contract,
    stellar_xdr::curr as xdr,
    std::{
        collections::HashMap,
        sync::Mutex,
        time::{Duration, Instant},
    },
};

const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";
const VAULT_BALANCE_CACHE_TTL: Duration = Duration::from_secs(30);

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

/// Cached vault SAC balances (TTL [`VAULT_BALANCE_CACHE_TTL`]).
#[derive(Default)]
pub struct VaultBalanceCache {
    inner: Mutex<HashMap<String, (u128, Instant)>>,
}

impl VaultBalanceCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get(&self, rpc_url: &str, vault: &str, token: &str) -> Result<u128> {
        let key = format!("{vault}|{token}");
        if let Some(bal) = self.cached(&key) {
            return Ok(bal);
        }
        let bal = fetch_token_balance_stroops(rpc_url, token, vault).await?;
        if let Ok(mut g) = self.inner.lock() {
            g.insert(key, (bal, Instant::now()));
        }
        Ok(bal)
    }

    fn cached(&self, key: &str) -> Option<u128> {
        let g = self.inner.lock().ok()?;
        let (bal, at) = *g.get(key)?;
        (at.elapsed() < VAULT_BALANCE_CACHE_TTL).then_some(bal)
    }
}

/// Trade-size ceiling: `min(ARB_MAX_AMOUNT_IN, vault_base_balance)` when vault
/// is configured; otherwise config only.
pub async fn resolve_max_amount_in(ctx: &ArbContext, base_token: &str) -> u128 {
    let cfg_max = ctx.config.max_amount_in;
    let Some(vault) = ctx.config.vault_contract.as_deref() else {
        return cfg_max;
    };
    match ctx
        .vault_balances
        .get(&ctx.config.rpc_url, vault, base_token)
        .await
    {
        Ok(bal) => cfg_max.min(bal),
        Err(e) => {
            tracing::warn!(
                error = %e,
                vault,
                token = base_token,
                "vault balance read failed — using ARB_MAX_AMOUNT_IN only"
            );
            cfg_max
        }
    }
}
