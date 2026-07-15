//! Build `aggregator::round_trip_swap` Soroban invoke operations.

use {
    crate::quote_client::LegQuote,
    anyhow::{anyhow, Context, Result},
    market_snapshot::MarketSnapshot,
    router_engine::{OptimalRoute, Path, QuoteHydration, TokenId},
    stellar_strkey::ed25519::PublicKey,
    stellar_xdr::curr::{self as xdr, Limits, WriteXdr},
};

/// One hop encoded for the on-chain `SwapStep` struct.
#[derive(Debug, Clone)]
pub struct ArbSwapStep {
    pub dex_type: String,
    pub pool_address: String,
    pub token_in: String,
    pub token_out: String,
    pub in_idx: u32,
    pub out_idx: u32,
}

pub fn source_to_dex_type(source: &str) -> Result<&'static str> {
    match source {
        // Aquarius xy=k, stableswap, and CLMM pools share swap(user, in_idx, out_idx, ...).
        "aquarius" | "aquarius_clmm" => Ok("aquarius"),
        "soroswap" => Ok("soroswap"),
        "phoenix" => Ok("phoenix"),
        "sushi" => Ok("sushi"),
        "comet" => Ok("comet"),
        other => Err(anyhow!("unsupported dex source: {}", other)),
    }
}

/// Resolve pool token indices for a single hop.
pub fn resolve_hop_indices(
    snapshot: &MarketSnapshot,
    hydration: &QuoteHydration,
    source: &str,
    pool_address: &str,
    token_in: &TokenId,
    token_out: &TokenId,
) -> Result<(u32, u32)> {
    let in_key = token_in.canonical();
    let out_key = token_out.canonical();

    if source == "aquarius" {
        if let Some(state) = hydration.aquarius_pools.get(pool_address) {
            let in_idx = state
                .tokens
                .iter()
                .position(|t| t == &in_key)
                .context("token_in not in aquarius pool")? as u32;
            let out_idx = state
                .tokens
                .iter()
                .position(|t| t == &out_key)
                .context("token_out not in aquarius pool")? as u32;
            return Ok((in_idx, out_idx));
        }
    }

    let pair = snapshot
        .sources
        .iter()
        .find(|s| s.source == source)
        .and_then(|s| s.pairs.iter().find(|p| p.pool_address == pool_address))
        .with_context(|| format!("pool {} not found in snapshot source {}", pool_address, source))?;

    let token_a = TokenId::from_str_auto(&pair.token_a).canonical();
    let token_b = TokenId::from_str_auto(&pair.token_b).canonical();

    if in_key == token_a && out_key == token_b {
        Ok((0, 1))
    } else if in_key == token_b && out_key == token_a {
        Ok((1, 0))
    } else {
        Err(anyhow!(
            "tokens {} -> {} do not match pool {} ({}, {})",
            in_key,
            out_key,
            pool_address,
            token_a,
            token_b
        ))
    }
}

/// Convert a router path into contract `SwapStep` arguments.
pub fn path_to_steps(path: &Path, snapshot: &MarketSnapshot, hydration: &QuoteHydration) -> Result<Vec<ArbSwapStep>> {
    if path.sources.len() != path.pool_addresses.len() || path.tokens.len() < 2 {
        return Err(anyhow!("invalid path shape"));
    }
    let hop_count = path.sources.len();
    let mut steps = Vec::with_capacity(hop_count);

    for i in 0..hop_count {
        let source = &path.sources[i];
        let pool = &path.pool_addresses[i];
        let token_in = &path.tokens[i];
        let token_out = &path.tokens[i + 1];
        let dex_type = source_to_dex_type(source)?.to_string();
        let (in_idx, out_idx) = resolve_hop_indices(snapshot, hydration, source, pool, token_in, token_out)?;

        steps.push(ArbSwapStep {
            dex_type,
            pool_address: pool.clone(),
            token_in: token_in.canonical(),
            token_out: token_out.canonical(),
            in_idx,
            out_idx,
        });
    }

    Ok(steps)
}

fn dex_type_scval(dex_type: &str) -> Result<xdr::ScVal> {
    let name = match dex_type {
        "aquarius" => "Aquarius",
        "soroswap" => "SoroswapPair",
        "phoenix" => "Phoenix",
        "sushi" => "Sushi",
        "comet" => "CometDex",
        other => return Err(anyhow!("unknown dex_type {}", other)),
    };
    Ok(xdr::ScVal::Vec(Some(xdr::ScVec(
        vec![xdr::ScVal::Symbol(xdr::ScSymbol(
            name.try_into().map_err(|_| anyhow!("bad dex symbol"))?,
        ))]
        .try_into()
        .map_err(|_| anyhow!("dex enum vec"))?,
    ))))
}

fn contract_hash(contract: &str) -> Result<[u8; 32]> {
    Ok(stellar_strkey::Contract::from_string(contract)
        .with_context(|| format!("invalid contract id {}", contract))?
        .0)
}

fn i128_scval(v: i128) -> xdr::ScVal {
    xdr::ScVal::I128(xdr::Int128Parts {
        hi: (v >> 64) as i64,
        lo: v as u64,
    })
}

fn step_to_scval(step: &ArbSwapStep) -> Result<xdr::ScVal> {
    let pool_hash = contract_hash(&step.pool_address)?;
    let token_in_hash = contract_hash(&step.token_in)?;
    let token_out_hash = contract_hash(&step.token_out)?;

    Ok(xdr::ScVal::Map(Some(xdr::ScMap(
        vec![
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("dex_id".try_into().unwrap())),
                val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(pool_hash)))),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("dex_type".try_into().unwrap())),
                val: dex_type_scval(&step.dex_type)?,
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("in_idx".try_into().unwrap())),
                val: xdr::ScVal::U32(step.in_idx),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("out_idx".try_into().unwrap())),
                val: xdr::ScVal::U32(step.out_idx),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("token_in".try_into().unwrap())),
                val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_in_hash)))),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("token_out".try_into().unwrap())),
                val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_out_hash)))),
            },
        ]
        .try_into()
        .map_err(|_| anyhow!("step map too large"))?,
    ))))
}

fn sum_sub_order_amounts(route: &OptimalRoute) -> u128 {
    route.sub_orders.iter().map(|s| s.amount_in).sum()
}

/// `round_trip_swap` requires each leg's sub-route `amount_in` values to sum to
/// the leg total exactly (base input for leg_out, bridge output for leg_back).
fn normalize_sub_order_amounts(route: &mut OptimalRoute, target_total: u128) {
    if route.sub_orders.is_empty() || target_total == 0 {
        return;
    }
    let current = sum_sub_order_amounts(route);
    if current == target_total {
        return;
    }
    if current == 0 {
        if route.sub_orders.len() == 1 {
            route.sub_orders[0].amount_in = target_total;
        }
        return;
    }

    let n = route.sub_orders.len();
    let mut allocated = 0u128;
    for (i, sub) in route.sub_orders.iter_mut().enumerate() {
        if i + 1 == n {
            sub.amount_in = target_total.saturating_sub(allocated);
        } else {
            let scaled = (sub.amount_in.saturating_mul(target_total) / current).max(1);
            sub.amount_in = scaled;
            allocated += scaled;
        }
    }

    let final_sum = sum_sub_order_amounts(route);
    if final_sum > target_total {
        let excess = final_sum - target_total;
        if let Some(last) = route.sub_orders.last_mut() {
            last.amount_in = last.amount_in.saturating_sub(excess);
        }
    }
}

/// Prepare on-chain leg payloads.
///
/// - `leg_out.amount_in` values are normalized to sum exactly to `amount_in`.
/// - `leg_back.amount_in` values are **weights** (quoted bridge amounts). The
///   aggregator rescales them to the actual on-chain bridge total after
///   `leg_out`; callers need not match `o1` exactly.
pub fn prepare_round_trip_routes(
    amount_in: i128,
    leg_out: &OptimalRoute,
    leg_back: &OptimalRoute,
) -> (OptimalRoute, OptimalRoute) {
    let mut out = leg_out.clone();
    let mut back = leg_back.clone();
    normalize_sub_order_amounts(&mut out, amount_in as u128);
    // Prefer per-sub-route quoted bridge outs as weights when split counts align.
    if out.sub_orders.len() == back.sub_orders.len() && out.sub_orders.len() > 1 {
        for (o, b) in out.sub_orders.iter().zip(back.sub_orders.iter_mut()) {
            if o.expected_amount_out > 0 {
                b.amount_in = o.expected_amount_out;
            }
        }
    }
    for b in back.sub_orders.iter_mut() {
        if b.amount_in == 0 {
            b.amount_in = 1;
        }
    }
    (out, back)
}

fn leg_to_sub_routes_scval(prepared_route: &OptimalRoute, step_sets: &[Vec<ArbSwapStep>]) -> Result<xdr::ScVal> {
    if prepared_route.sub_orders.len() != step_sets.len() {
        return Err(anyhow!(
            "sub_order count {} != step_sets count {}",
            prepared_route.sub_orders.len(),
            step_sets.len()
        ));
    }

    let mut sub_routes = Vec::with_capacity(prepared_route.sub_orders.len());
    for (sub, steps) in prepared_route.sub_orders.iter().zip(step_sets.iter()) {
        let steps_scval: Vec<xdr::ScVal> = steps.iter().map(step_to_scval).collect::<Result<_>>()?;

        let amount_in_entry = xdr::ScMapEntry {
            key: xdr::ScVal::Symbol(xdr::ScSymbol("amount_in".try_into().unwrap())),
            val: i128_scval(sub.amount_in as i128),
        };
        let steps_entry = xdr::ScMapEntry {
            key: xdr::ScVal::Symbol(xdr::ScSymbol("steps".try_into().unwrap())),
            val: xdr::ScVal::Vec(Some(steps_scval.try_into().map_err(|_| anyhow!("too many steps"))?)),
        };

        sub_routes.push(xdr::ScVal::Map(Some(xdr::ScMap(
            vec![amount_in_entry, steps_entry]
                .try_into()
                .map_err(|_| anyhow!("sub_route map"))?,
        ))));
    }

    Ok(xdr::ScVal::Vec(Some(
        sub_routes.try_into().map_err(|_| anyhow!("too many sub_routes"))?,
    )))
}

/// Build `InvokeHostFunction` calling `aggregator.round_trip_swap`.
pub fn build_round_trip_swap_op(
    aggregator_contract: &str,
    user_public_key: &str,
    base_token: &str,
    bridge_token: &str,
    amount_in: i128,
    leg_out: &LegQuote,
    leg_back: &LegQuote,
    min_amount_out: i128,
) -> Result<xdr::Operation> {
    if amount_in <= 0 {
        return Err(anyhow!("amount_in must be positive"));
    }
    if min_amount_out < amount_in {
        return Err(anyhow!("min_amount_out below principal"));
    }
    if leg_out.route.sub_orders.is_empty() || leg_back.route.sub_orders.is_empty() {
        return Err(anyhow!("round_trip_swap requires non-empty legs"));
    }

    let user_key = PublicKey::from_string(user_public_key)
        .with_context(|| format!("invalid user public key {}", user_public_key))?;
    let agg_hash = contract_hash(aggregator_contract)?;
    let base_hash = contract_hash(base_token)?;
    let bridge_hash = contract_hash(bridge_token)?;

    let (prepared_out, prepared_back) = prepare_round_trip_routes(amount_in, &leg_out.route, &leg_back.route);

    let leg_out_val = leg_to_sub_routes_scval(&prepared_out, &leg_out.step_sets)?;
    let leg_back_val = leg_to_sub_routes_scval(&prepared_back, &leg_back.step_sets)?;

    let invoke_args = xdr::InvokeContractArgs {
        contract_address: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(agg_hash))),
        function_name: xdr::ScSymbol("round_trip_swap".try_into().unwrap()),
        args: vec![
            xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
                xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(user_key.0)),
            ))),
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(base_hash)))),
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(bridge_hash)))),
            i128_scval(amount_in),
            leg_out_val,
            leg_back_val,
            i128_scval(min_amount_out),
        ]
        .try_into()
        .map_err(|_| anyhow!("round_trip_swap args"))?,
    };

    Ok(xdr::Operation {
        source_account: None,
        body: xdr::OperationBody::InvokeHostFunction(xdr::InvokeHostFunctionOp {
            host_function: xdr::HostFunction::InvokeContract(invoke_args),
            auth: xdr::VecM::default(),
        }),
    })
}

/// Build `InvokeHostFunction` calling `vault.execute_round_trip`.
pub fn build_execute_round_trip_op(
    vault_contract: &str,
    aggregator_contract: &str,
    caller_public_key: &str,
    base_token: &str,
    bridge_token: &str,
    amount_in: i128,
    leg_out: &LegQuote,
    leg_back: &LegQuote,
    min_amount_out: i128,
) -> Result<xdr::Operation> {
    if amount_in <= 0 {
        return Err(anyhow!("amount_in must be positive"));
    }
    if min_amount_out < amount_in {
        return Err(anyhow!("min_amount_out below principal"));
    }
    if leg_out.route.sub_orders.is_empty() || leg_back.route.sub_orders.is_empty() {
        return Err(anyhow!("round_trip_swap requires non-empty legs"));
    }

    let caller_key = PublicKey::from_string(caller_public_key)
        .with_context(|| format!("invalid caller public key {}", caller_public_key))?;
    let vault_hash = contract_hash(vault_contract)?;
    let agg_hash = contract_hash(aggregator_contract)?;
    let base_hash = contract_hash(base_token)?;
    let bridge_hash = contract_hash(bridge_token)?;

    let (prepared_out, prepared_back) = prepare_round_trip_routes(amount_in, &leg_out.route, &leg_back.route);

    let leg_out_val = leg_to_sub_routes_scval(&prepared_out, &leg_out.step_sets)?;
    let leg_back_val = leg_to_sub_routes_scval(&prepared_back, &leg_back.step_sets)?;

    let invoke_args = xdr::InvokeContractArgs {
        contract_address: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(vault_hash))),
        function_name: xdr::ScSymbol("execute_round_trip".try_into().unwrap()),
        args: vec![
            xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
                xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(caller_key.0)),
            ))),
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(agg_hash)))),
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(base_hash)))),
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(bridge_hash)))),
            i128_scval(amount_in),
            leg_out_val,
            leg_back_val,
            i128_scval(min_amount_out),
        ]
        .try_into()
        .map_err(|_| anyhow!("execute_round_trip args"))?,
    };

    Ok(xdr::Operation {
        source_account: None,
        body: xdr::OperationBody::InvokeHostFunction(xdr::InvokeHostFunctionOp {
            host_function: xdr::HostFunction::InvokeContract(invoke_args),
            auth: xdr::VecM::default(),
        }),
    })
}

/// On-chain floor: base output must exceed input (arb only cares that XLM
/// grows).
pub fn min_amount_out_break_even(amount_in: u128) -> i128 {
    amount_in.saturating_add(1).min(i128::MAX as u128) as i128
}

pub fn build_raw_envelope_xdr(source_public_key: &str, sequence: u64, op: xdr::Operation) -> Result<String> {
    let pk = PublicKey::from_string(source_public_key)
        .with_context(|| format!("invalid source key {}", source_public_key))?;
    let source_account = xdr::MuxedAccount::Ed25519(xdr::Uint256(pk.0));
    let tx = xdr::Transaction {
        source_account,
        fee: 100_000,
        seq_num: xdr::SequenceNumber(sequence as i64),
        cond: xdr::Preconditions::None,
        memo: xdr::Memo::None,
        operations: vec![op].try_into().map_err(|_| anyhow!("ops"))?,
        ext: xdr::TransactionExt::V0,
    };
    let envelope = xdr::TransactionEnvelope::Tx(xdr::TransactionV1Envelope {
        tx,
        signatures: xdr::VecM::default(),
    });
    envelope
        .to_xdr_base64(Limits::none())
        .context("encode transaction envelope")
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        market_snapshot::{SourceSnapshot, TradingPairSnapshot},
    };

    #[test]
    fn normalize_leg_back_splits_to_bridge_total() {
        use router_engine::{OptimalRoute, SubOrder};

        let path = router_engine::Path {
            hops: 1,
            tokens: vec![TokenId::from_str_auto("A"), TokenId::from_str_auto("B")],
            sources: vec!["soroswap".into()],
            pool_addresses: vec!["p1".into()],
        };
        let mut leg_back = OptimalRoute {
            sub_orders: vec![
                SubOrder {
                    path: path.clone(),
                    amount_in: 9370191,
                    expected_amount_out: 0,
                    fraction: 0.0,
                },
                SubOrder {
                    path: path.clone(),
                    amount_in: 7211851,
                    expected_amount_out: 0,
                    fraction: 0.0,
                },
                SubOrder {
                    path,
                    amount_in: 1674582,
                    expected_amount_out: 0,
                    fraction: 0.0,
                },
            ],
            total_amount_in: 0,
            total_expected_out: 0,
            price_impact_bps: 0,
            is_split: true,
            improvement_bps: 0,
            minimum_out: 0,
            compute_time_ms: 0,
            debug: None,
        };
        normalize_sub_order_amounts(&mut leg_back, 18_606_031);
        assert_eq!(sum_sub_order_amounts(&leg_back), 18_606_031);
    }

    #[test]
    fn min_amount_out_requires_positive_return() {
        assert_eq!(min_amount_out_break_even(1_000_000_000), 1_000_000_001);
    }

    #[test]
    fn maps_two_token_hop_indices() {
        let snapshot = market_snapshot::MarketSnapshot::from_sources(
            "v1",
            0,
            "test",
            vec![SourceSnapshot {
                source: "soroswap".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".into(),
                    token_b: "B".into(),
                    pool_address: "p1".into(),
                    fee_bps: 30,
                }],
            }],
        );
        let path = router_engine::Path {
            hops: 1,
            tokens: vec![TokenId::from_str_auto("A"), TokenId::from_str_auto("B")],
            sources: vec!["soroswap".into()],
            pool_addresses: vec!["p1".into()],
        };
        let hydration = QuoteHydration::default();
        let steps = path_to_steps(&path, &snapshot, &hydration).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].in_idx, 0);
        assert_eq!(steps[0].out_idx, 1);
    }
}
