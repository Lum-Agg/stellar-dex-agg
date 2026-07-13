//! Sweep non-native balances from caller accounts 1–9 to index 0, then close
//! trustlines.
//!
//! Usage on server:
//!   MNEMONIC_PATH=/etc/mnemonic_code.txt \
//!   HORIZON_URL=http://127.0.0.1:8000 \
//!   cargo run --release -p arbitrage --bin clean_caller_trustlines
//!
//! Preview only:
//!   DRY_RUN=1 ...

use {
    anyhow::{anyhow, Context, Result},
    std::{collections::HashSet, env, time::Duration},
    stellar_baselib::{
        account::{Account, AccountBehavior},
        asset::{Asset, AssetBehavior},
        keypair::{Keypair, KeypairBehavior},
        liquidity_pool_asset::{LiquidityPoolAsset, LiquidityPoolAssetBehavior},
        network::{NetworkPassphrase, Networks},
        operation::Operation,
        transaction::TransactionBehavior,
        transaction_builder::{TransactionBuilder, TransactionBuilderBehavior, TIMEOUT_INFINITE},
        xdr::{self, Limits, WriteXdr},
    },
};

const BASE_FEE: u32 = 100;
const MAX_OPS_PER_TX: usize = 50;

#[derive(Debug, Clone)]
enum TrustLineKind {
    Credit { asset: Asset },
    PoolShare { pool_id: [u8; 32] },
}

#[derive(Debug, Clone)]
struct BalanceEntry {
    kind: TrustLineKind,
    balance_stroops: i64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let mnemonic_path = env::var("MNEMONIC_PATH").unwrap_or_else(|_| "/etc/mnemonic_code.txt".into());
    let horizon_url = env::var("HORIZON_URL").unwrap_or_else(|_| "https://horizon.stellar.org".into());
    let dry_run = env::var("DRY_RUN").ok().as_deref() == Some("1");
    let sweep = env::var("SWEEP").ok().as_deref() != Some("0");
    let dest_index: u32 = env::var("DESTINATION_INDEX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let start_index: u32 = env::var("SOURCE_START").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let end_index: u32 = env::var("SOURCE_END").ok().and_then(|s| s.parse().ok()).unwrap_or(9);

    let phrase = std::fs::read_to_string(&mnemonic_path)
        .with_context(|| format!("read mnemonic {}", mnemonic_path))?
        .trim()
        .to_string();

    let dest_kp = mnemonic_keypair(&phrase, dest_index)?;
    let dest_g = dest_kp.public_key();
    if sweep {
        println!("destination index={dest_index} {dest_g}");
    } else {
        println!("sweep disabled — only closing zero-balance trustlines");
    }

    let mut dest_trustlines = if sweep {
        let keys = fetch_trustline_keys(&horizon_url, &dest_g).await?;
        println!("destination has {} trustlines", keys.len());
        keys
    } else {
        HashSet::new()
    };

    if sweep {
        // Ensure destination can receive any credit assets we will sweep.
        let mut assets_to_receive = HashSet::new();
        for idx in start_index..=end_index {
            let kp = mnemonic_keypair(&phrase, idx)?;
            let entries = fetch_balances(&horizon_url, &kp.public_key()).await?;
            for entry in &entries {
                if let TrustLineKind::Credit { asset } = &entry.kind {
                    if entry.balance_stroops > 0 {
                        assets_to_receive.insert(asset_key(asset));
                    }
                }
            }
        }

        let missing: Vec<Asset> = assets_to_receive
            .iter()
            .filter(|k| !dest_trustlines.contains(*k))
            .filter_map(|k| asset_from_key(k))
            .collect();

        if !missing.is_empty() {
            println!("adding {} trustlines on destination", missing.len());
            let seq = fetch_sequence(&horizon_url, &dest_g).await?;
            let mut account = Account::new(&dest_g, &seq.to_string()).map_err(|e| anyhow!(e))?;
            submit_batched_change_trust_add(&horizon_url, &dest_kp, &mut account, &missing, dry_run).await?;
            for asset in &missing {
                dest_trustlines.insert(asset_key(asset));
            }
        }
    }

    for idx in start_index..=end_index {
        let kp = mnemonic_keypair(&phrase, idx)?;
        let g = kp.public_key();
        let entries = fetch_balances(&horizon_url, &g).await?;
        let trustlines = entries.len().saturating_sub(native_count(&entries));
        println!("\n=== account index={idx} {g} ({trustlines} trustlines) ===");

        let non_native: Vec<_> = entries.iter().filter(|e| !is_native_only(e)).collect();
        if non_native.is_empty() {
            println!("no trustlines, skip");
            continue;
        }
        let seq = fetch_sequence(&horizon_url, &g).await?;
        let mut account = Account::new(&g, &seq.to_string()).map_err(|e| anyhow!(e))?;

        // Horizon may truncate embedded balances; loop sweep+close until clean.
        for pass in 1..=20 {
            let entries = fetch_balances(&horizon_url, &g).await?;
            let trustlines = entries.iter().filter(|e| !is_native_only(e)).count();
            if trustlines == 0 {
                if pass == 1 {
                    println!("no trustlines, skip");
                }
                break;
            }
            if pass == 1 {
                println!("({trustlines} trustlines visible this pass)");
            } else {
                println!("pass {pass}: {trustlines} trustlines remain");
            }

            let payments: Vec<(Asset, i64)> = if sweep {
                entries
                    .iter()
                    .filter_map(|e| match &e.kind {
                        TrustLineKind::Credit { asset } if !asset.is_native() && e.balance_stroops > 0 => {
                            Some((asset.clone(), e.balance_stroops))
                        }
                        _ => None,
                    })
                    .collect()
            } else {
                Vec::new()
            };

            for (asset, amount) in &payments {
                let ops = vec![Operation::new()
                    .payment(&dest_g, asset, *amount)
                    .map_err(|e| anyhow!("payment op: {:?}", e))?];
                let label = format!("sweep index={idx} {} {}", asset.get_code().unwrap_or_default(), *amount);
                if let Err(e) = submit_ops(&horizon_url, &kp, &mut account, &ops, &label, dry_run).await {
                    eprintln!("WARN {label}: {e:#}");
                    let seq = fetch_sequence(&horizon_url, &g).await?;
                    account = Account::new(&g, &seq.to_string()).map_err(|e| anyhow!(e))?;
                }
            }

            let entries = fetch_balances(&horizon_url, &g).await?;
            let mut close_ops: Vec<xdr::Operation> = Vec::new();
            for entry in &entries {
                match &entry.kind {
                    TrustLineKind::Credit { asset } => {
                        if asset.is_native() {
                            continue;
                        }
                        if entry.balance_stroops > 0 {
                            continue;
                        }
                        close_ops.push(
                            Operation::new()
                                .change_trust(asset, 0i64)
                                .map_err(|e| anyhow!("change_trust op: {:?}", e))?,
                        );
                    }
                    TrustLineKind::PoolShare { pool_id } => {
                        if entry.balance_stroops > 0 {
                            eprintln!(
                                "WARN index={idx}: pool share {} has balance {}, withdraw manually",
                                hex::encode(pool_id),
                                entry.balance_stroops
                            );
                            continue;
                        }
                        let ct_asset = pool_change_trust_asset(&horizon_url, pool_id).await?;
                        close_ops.push(
                            Operation::new()
                                .change_trust(ct_asset, 0i64)
                                .map_err(|e| anyhow!("change_trust pool op: {:?}", e))?,
                        );
                    }
                }
            }

            if close_ops.is_empty() {
                let with_balance = entries.iter().filter(|e| !is_native_only(e) && e.balance_stroops > 0).count();
                if with_balance > 0 {
                    println!("index={idx}: {with_balance} trustlines kept (non-zero balance)");
                } else {
                    eprintln!("WARN index={idx}: no closable trustlines on pass {pass}, stopping");
                }
                break;
            }

            for chunk in close_ops.chunks(MAX_OPS_PER_TX) {
                let label = format!("close index={idx} pass={pass} ({} trustlines)", chunk.len());
                if let Err(e) = submit_ops(&horizon_url, &kp, &mut account, chunk, &label, dry_run).await {
                    eprintln!("WARN {label}: {e:#}");
                    let seq = fetch_sequence(&horizon_url, &g).await?;
                    account = Account::new(&g, &seq.to_string()).map_err(|e| anyhow!(e))?;
                }
            }
        }

        let remaining = fetch_balances(&horizon_url, &g).await?;
        let remaining_tl = remaining.iter().filter(|e| !is_native_only(e)).count();
        println!("done index={idx}: {remaining_tl} trustlines remain");
    }

    Ok(())
}

fn is_native_only(entry: &BalanceEntry) -> bool {
    matches!(entry.kind, TrustLineKind::Credit { ref asset } if asset.is_native())
}

fn native_count(entries: &[BalanceEntry]) -> usize {
    entries.iter().filter(|e| is_native_only(e)).count()
}

fn asset_key(asset: &Asset) -> String {
    format!(
        "{}:{}",
        asset.get_code().unwrap_or_default(),
        asset.get_issuer().unwrap_or_default()
    )
}

fn asset_from_key(key: &str) -> Option<Asset> {
    let (code, issuer) = key.split_once(':')?;
    Asset::new(code, Some(issuer)).ok()
}

async fn fetch_sequence(horizon_url: &str, public_key: &str) -> Result<i64> {
    let url = format!("{}/accounts/{}", horizon_url.trim_end_matches('/'), public_key);
    let data: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let seq_str = data
        .get("sequence")
        .and_then(|s| s.as_str())
        .ok_or_else(|| anyhow!("missing sequence"))?;
    seq_str.parse().map_err(|e| anyhow!("parse sequence: {e}"))
}

async fn fetch_trustline_keys(horizon_url: &str, public_key: &str) -> Result<HashSet<String>> {
    let entries = fetch_balances(horizon_url, public_key).await?;
    Ok(entries
        .into_iter()
        .filter_map(|e| match e.kind {
            TrustLineKind::Credit { asset } if !asset.is_native() => Some(asset_key(&asset)),
            _ => None,
        })
        .collect())
}

async fn fetch_balances(horizon_url: &str, public_key: &str) -> Result<Vec<BalanceEntry>> {
    let url = format!("{}/accounts/{}", horizon_url.trim_end_matches('/'), public_key);
    let data: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let balances = data
        .get("balances")
        .and_then(|b| b.as_array())
        .ok_or_else(|| anyhow!("missing balances"))?;

    let mut out = Vec::new();
    for bal in balances {
        let asset_type = bal.get("asset_type").and_then(|v| v.as_str()).unwrap_or("");
        let balance_stroops = balance_to_stroops(bal.get("balance").and_then(|v| v.as_str()).unwrap_or("0"))?;

        match asset_type {
            "native" => {
                out.push(BalanceEntry {
                    kind: TrustLineKind::Credit { asset: Asset::native() },
                    balance_stroops,
                });
            }
            "credit_alphanum4" | "credit_alphanum12" => {
                let code = bal.get("asset_code").and_then(|v| v.as_str()).context("asset_code")?;
                let issuer = bal
                    .get("asset_issuer")
                    .and_then(|v| v.as_str())
                    .context("asset_issuer")?;
                let asset = Asset::new(code, Some(issuer)).map_err(|e| anyhow!(e))?;
                out.push(BalanceEntry {
                    kind: TrustLineKind::Credit { asset },
                    balance_stroops,
                });
            }
            "liquidity_pool_shares" => {
                let pool_id_hex = bal
                    .get("liquidity_pool_id")
                    .and_then(|v| v.as_str())
                    .context("liquidity_pool_id")?;
                let pool_id = decode_pool_id(pool_id_hex)?;
                out.push(BalanceEntry {
                    kind: TrustLineKind::PoolShare { pool_id },
                    balance_stroops,
                });
            }
            other => eprintln!("skip unknown asset_type {other}"),
        }
    }
    Ok(out)
}

fn decode_pool_id(hex_str: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex_str).context("decode pool id hex")?;
    bytes.try_into().map_err(|_| anyhow!("pool id must be 32 bytes"))
}

async fn pool_change_trust_asset(horizon_url: &str, pool_id: &[u8; 32]) -> Result<xdr::ChangeTrustAsset> {
    let pool_id_hex = hex::encode(pool_id);
    let url = format!("{}/liquidity_pools/{}", horizon_url.trim_end_matches('/'), pool_id_hex);
    let data: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let fee = data.get("fee_bp").and_then(|v| v.as_i64()).unwrap_or(30) as i32;
    let reserves = data
        .get("reserves")
        .and_then(|v| v.as_array())
        .context("pool reserves")?;
    if reserves.len() < 2 {
        return Err(anyhow!("pool {pool_id_hex} missing reserves"));
    }

    let asset_a = parse_horizon_asset(
        reserves[0]
            .get("asset")
            .and_then(|v| v.as_str())
            .context("reserve asset_a")?,
    )?;
    let asset_b = parse_horizon_asset(
        reserves[1]
            .get("asset")
            .and_then(|v| v.as_str())
            .context("reserve asset_b")?,
    )?;

    if fee == 30 {
        let lp = LiquidityPoolAsset::new(asset_a, asset_b, fee).map_err(|e| anyhow!(e))?;
        return Ok(lp.to_xdr_object());
    }

    Ok(xdr::ChangeTrustAsset::PoolShare(
        xdr::LiquidityPoolParameters::LiquidityPoolConstantProduct(xdr::LiquidityPoolConstantProductParameters {
            asset_a: asset_a.to_xdr_object(),
            asset_b: asset_b.to_xdr_object(),
            fee,
        }),
    ))
}

fn parse_horizon_asset(raw: &str) -> Result<Asset> {
    if raw == "native" {
        return Ok(Asset::native());
    }
    let (code, issuer) = raw.split_once(':').context("asset CODE:ISSUER")?;
    Asset::new(code, Some(issuer)).map_err(|e| anyhow!(e))
}

fn balance_to_stroops(balance: &str) -> Result<i64> {
    let balance = balance.trim();
    if balance.is_empty() || balance == "0" {
        return Ok(0);
    }
    let (whole, frac) = match balance.split_once('.') {
        Some((w, f)) => (w, f),
        None => (balance, ""),
    };
    let whole: i64 = whole.parse().context("whole part")?;
    let mut frac_str: String = frac.chars().take(7).collect();
    while frac_str.len() < 7 {
        frac_str.push('0');
    }
    let frac: i64 = frac_str.parse().context("frac part")?;
    whole
        .checked_mul(10_000_000)
        .and_then(|w| w.checked_add(frac))
        .ok_or_else(|| anyhow!("balance overflow"))
}

async fn submit_batched_change_trust_add(
    horizon_url: &str,
    kp: &Keypair,
    account: &mut Account,
    assets: &[Asset],
    dry_run: bool,
) -> Result<()> {
    for chunk in assets.chunks(MAX_OPS_PER_TX) {
        let ops: Vec<xdr::Operation> = chunk
            .iter()
            .map(|asset| {
                Operation::new()
                    .change_trust(asset, None::<i64>)
                    .map_err(|e| anyhow!("change_trust add: {:?}", e))
            })
            .collect::<Result<_>>()?;
        let label = format!("add trustlines on {} ({} ops)", kp.public_key(), chunk.len());
        submit_ops(horizon_url, kp, account, &ops, &label, dry_run).await?;
    }
    Ok(())
}

async fn submit_ops(
    horizon_url: &str,
    kp: &Keypair,
    account: &mut Account,
    ops: &[xdr::Operation],
    label: &str,
    dry_run: bool,
) -> Result<()> {
    if ops.is_empty() {
        return Ok(());
    }

    let fee = BASE_FEE;
    let mut builder = TransactionBuilder::new(account, Networks::public(), None);
    builder.fee(fee);
    for op in ops {
        builder.add_operation(op.clone());
    }
    let mut tx = builder.set_timeout(TIMEOUT_INFINITE).map_err(|e| anyhow!(e))?.build();
    tx.sign(&[kp.clone()]);

    let envelope = tx.to_envelope().map_err(|e| anyhow!("to_envelope: {e}"))?;
    let xdr_b64 = envelope
        .to_xdr_base64(Limits::none())
        .map_err(|e| anyhow!("xdr encode: {:?}", e))?;

    if dry_run {
        println!("DRY_RUN {label}: fee={} ops={}", fee * ops.len() as u32, ops.len());
        account.increment_sequence_number();
        return Ok(());
    }

    println!("submit {label} ...");
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/transactions", horizon_url.trim_end_matches('/')))
        .form(&[("tx", xdr_b64)])
        .send()
        .await?;

    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(anyhow!("Horizon submit failed ({status}): {body}"));
    }

    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::json!({}));
    let hash = parsed.get("hash").and_then(|h| h.as_str()).unwrap_or("?");
    println!("SUCCESS {label} hash={hash}");
    tokio::time::sleep(Duration::from_secs(3)).await;
    Ok(())
}

fn mnemonic_keypair(phrase: &str, index: u32) -> Result<Keypair> {
    use bip39::{Language, Mnemonic};

    let mnemonic = Mnemonic::parse_in(Language::English, phrase).map_err(|e| anyhow!("invalid mnemonic: {e:?}"))?;
    let seed = mnemonic.to_seed("");
    let path = [0x8000_0000 + 44, 0x8000_0000 + 148, 0x8000_0000 + index];
    let derived = slip10_ed25519::derive_ed25519_private_key(&seed, &path);
    Keypair::from_raw_ed25519_seed(&derived).map_err(|e| anyhow!("derive keypair: {e:?}"))
}
