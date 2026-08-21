//! Parse aggregator contract invocations from Soroban transaction XDR.

use {
    anyhow::{anyhow, Context, Result},
    base64::{engine::general_purpose::STANDARD as BASE64, Engine as _},
    stellar_strkey::{ed25519::PublicKey, Contract},
    stellar_xdr::curr::{self as xdr, Limits, ReadXdr},
};

/// One DEX hop extracted from on-chain `SwapStep`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLeg {
    pub leg_index: u32,
    pub dex_source: String,
    pub pool_address: String,
    pub token_in: Option<String>,
    pub token_out: Option<String>,
    pub amount_in: Option<i128>,
    pub amount_out: Option<i128>,
    pub amount_is_actual: bool,
}

/// Parsed aggregator invocation (swap or round_trip_swap).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedInvocation {
    pub function_name: String,
    pub user_address: String,
    pub token_in: Option<String>,
    pub token_out: Option<String>,
    pub bridge_token: Option<String>,
    pub amount_in: i128,
    pub amount_out: Option<i128>,
    pub is_split: bool,
    pub legs: Vec<ParsedLeg>,
}

/// Convert Soroban's transaction result into a stable, low-cardinality reason.
/// The raw diagnostic event XDR is useful for debugging, but is not suitable as
/// an analytics dimension because it can contain dynamic VM details.
pub fn classify_failure(result_xdr: Option<&str>) -> Option<String> {
    let result_xdr = result_xdr?;
    let result = xdr::TransactionResult::from_xdr_base64(result_xdr, Limits::none()).ok()?;
    let reason = match result.result {
        xdr::TransactionResultResult::TxFailed(results) => results.iter().find_map(|operation| match operation {
            xdr::OperationResult::OpInner(xdr::OperationResultTr::InvokeHostFunction(
                xdr::InvokeHostFunctionResult::Trapped,
            )) => Some("HOST_FUNCTION_TRAPPED"),
            xdr::OperationResult::OpInner(xdr::OperationResultTr::InvokeHostFunction(
                xdr::InvokeHostFunctionResult::ResourceLimitExceeded,
            )) => Some("HOST_FUNCTION_RESOURCE_LIMIT"),
            xdr::OperationResult::OpInner(xdr::OperationResultTr::InvokeHostFunction(
                xdr::InvokeHostFunctionResult::EntryArchived,
            )) => Some("HOST_FUNCTION_ENTRY_ARCHIVED"),
            xdr::OperationResult::OpInner(xdr::OperationResultTr::InvokeHostFunction(
                xdr::InvokeHostFunctionResult::Malformed,
            )) => Some("HOST_FUNCTION_MALFORMED"),
            xdr::OperationResult::OpInner(xdr::OperationResultTr::InvokeHostFunction(
                xdr::InvokeHostFunctionResult::InsufficientRefundableFee,
            )) => Some("HOST_FUNCTION_INSUFFICIENT_REFUNDABLE_FEE"),
            xdr::OperationResult::OpBadAuth => Some("OP_BAD_AUTH"),
            xdr::OperationResult::OpNoAccount => Some("OP_NO_ACCOUNT"),
            xdr::OperationResult::OpExceededWorkLimit => Some("OP_EXCEEDED_WORK_LIMIT"),
            _ => None,
        }),
        xdr::TransactionResultResult::TxTooEarly => Some("TX_TOO_EARLY"),
        xdr::TransactionResultResult::TxTooLate => Some("TX_TOO_LATE"),
        xdr::TransactionResultResult::TxBadSeq => Some("TX_BAD_SEQ"),
        xdr::TransactionResultResult::TxBadAuth => Some("TX_BAD_AUTH"),
        xdr::TransactionResultResult::TxInsufficientBalance => Some("TX_INSUFFICIENT_BALANCE"),
        xdr::TransactionResultResult::TxInsufficientFee => Some("TX_INSUFFICIENT_FEE"),
        xdr::TransactionResultResult::TxSorobanInvalid => Some("TX_SOROBAN_INVALID"),
        xdr::TransactionResultResult::TxMalformed => Some("TX_MALFORMED"),
        xdr::TransactionResultResult::TxInternalError => Some("TX_INTERNAL_ERROR"),
        xdr::TransactionResultResult::TxNotSupported => Some("TX_NOT_SUPPORTED"),
        xdr::TransactionResultResult::TxMissingOperation => Some("TX_MISSING_OPERATION"),
        xdr::TransactionResultResult::TxBadAuthExtra => Some("TX_BAD_AUTH_EXTRA"),
        xdr::TransactionResultResult::TxNoAccount => Some("TX_NO_ACCOUNT"),
        xdr::TransactionResultResult::TxBadSponsorship => Some("TX_BAD_SPONSORSHIP"),
        xdr::TransactionResultResult::TxBadMinSeqAgeOrGap => Some("TX_BAD_MIN_SEQ_AGE_OR_GAP"),
        xdr::TransactionResultResult::TxFeeBumpInnerFailed(_) => Some("TX_FEE_BUMP_INNER_FAILED"),
        _ => Some("TX_FAILED"),
    }?;
    Some(reason.to_string())
}

#[cfg(test)]
mod failure_tests {
    use super::classify_failure;

    #[test]
    fn classifies_soroban_trap_result() {
        let result = "AAAAAAAByAP/////AAAAAQAAAAAAAAAY/////gAAAAA=";
        assert_eq!(classify_failure(Some(result)).as_deref(), Some("HOST_FUNCTION_TRAPPED"));
    }
}

pub fn parse_envelope(
    envelope_xdr: &str,
    aggregator_contract: &str,
    result_meta_xdr: Option<&str>,
) -> Result<Option<ParsedInvocation>> {
    let bytes = BASE64
        .decode(envelope_xdr.trim())
        .context("decode envelope xdr base64")?;
    let envelope = xdr::TransactionEnvelope::from_xdr(&bytes, Limits::none()).context("decode transaction envelope")?;

    let tx = match envelope {
        xdr::TransactionEnvelope::Tx(v1) => v1.tx,
        xdr::TransactionEnvelope::TxV0(_) => {
            return Err(anyhow!("TxV0 envelopes are not supported"));
        }
        xdr::TransactionEnvelope::TxFeeBump(fb) => match fb.tx.inner_tx {
            xdr::FeeBumpTransactionInnerTx::Tx(v1) => v1.tx,
        },
    };

    let agg_hash = contract_hash(aggregator_contract)?;
    let amount_out = result_meta_xdr.and_then(parse_success_return_i128);

    for op in tx.operations.iter() {
        let xdr::OperationBody::InvokeHostFunction(invoke) = &op.body else {
            continue;
        };

        // Direct aggregator invoke (user → aggregator).
        if let xdr::HostFunction::InvokeContract(args) = &invoke.host_function {
            if let Some(parsed) = try_parse_aggregator_invoke(args, &agg_hash, amount_out)? {
                return Ok(Some(parsed));
            }
        }

        // Vault / nested auth: aggregator call lives in authorization tree.
        for auth in invoke.auth.iter() {
            if let Some(parsed) = find_aggregator_in_auth(&auth.root_invocation, &agg_hash, amount_out)? {
                return Ok(Some(parsed));
            }
        }
    }

    Ok(None)
}

fn try_parse_aggregator_invoke(
    args: &xdr::InvokeContractArgs,
    agg_hash: &[u8; 32],
    amount_out: Option<i128>,
) -> Result<Option<ParsedInvocation>> {
    let xdr::ScAddress::Contract(contract_id) = &args.contract_address else {
        return Ok(None);
    };
    if contract_id.0 .0 != *agg_hash {
        return Ok(None);
    }

    let function = args.function_name.to_string();
    match function.as_str() {
        "swap" => parse_swap(args, amount_out),
        "round_trip_swap" => parse_round_trip_swap(args, amount_out),
        _ => Ok(None),
    }
}

fn find_aggregator_in_auth(
    inv: &xdr::SorobanAuthorizedInvocation,
    agg_hash: &[u8; 32],
    amount_out: Option<i128>,
) -> Result<Option<ParsedInvocation>> {
    if let xdr::SorobanAuthorizedFunction::ContractFn(cfn) = &inv.function {
        if let Some(parsed) = try_parse_aggregator_invoke(cfn, agg_hash, amount_out)? {
            return Ok(Some(parsed));
        }
    }
    for sub in inv.sub_invocations.iter() {
        if let Some(parsed) = find_aggregator_in_auth(sub, agg_hash, amount_out)? {
            return Ok(Some(parsed));
        }
    }
    Ok(None)
}

fn parse_swap(args: &xdr::InvokeContractArgs, amount_out: Option<i128>) -> Result<Option<ParsedInvocation>> {
    if args.args.len() < 5 {
        return Err(anyhow!("swap expects 5 args, got {}", args.args.len()));
    }

    let user_address = scval_address_str(&args.args[0])?;
    let token_in = Some(scval_address_str(&args.args[1])?);
    let token_out = Some(scval_address_str(&args.args[2])?);
    let sub_routes = scval_vec(&args.args[3]).ok_or_else(|| anyhow!("swap sub_routes not a vec"))?;
    let is_split = sub_routes.len() > 1;

    let mut legs = Vec::new();
    let mut amount_in: i128 = 0;

    for sub_route in &sub_routes {
        let map = scval_map(&sub_route).ok_or_else(|| anyhow!("sub_route not a map"))?;
        let route_amount = get_map_i128(map, "amount_in").unwrap_or(0);
        amount_in = amount_in.saturating_add(route_amount);

        if let Some(steps) = get_map_field(map, "steps").and_then(scval_vec) {
            for (hop, step) in steps.iter().enumerate() {
                if let Some(step_map) = scval_map(step) {
                    legs.push(parse_step(step_map, hop as u32, Some(route_amount))?);
                }
            }
        }
    }

    Ok(Some(ParsedInvocation {
        function_name: "swap".into(),
        user_address,
        token_in,
        token_out,
        bridge_token: None,
        amount_in,
        amount_out,
        is_split,
        legs,
    }))
}

fn parse_round_trip_swap(args: &xdr::InvokeContractArgs, amount_out: Option<i128>) -> Result<Option<ParsedInvocation>> {
    if args.args.len() < 7 {
        return Err(anyhow!("round_trip_swap expects 7 args, got {}", args.args.len()));
    }

    let user_address = scval_address_str(&args.args[0])?;
    let base_token = scval_address_str(&args.args[1])?;
    let bridge_token = scval_address_str(&args.args[2])?;
    let amount_in = scval_i128(&args.args[3]).unwrap_or(0);

    let mut legs = Vec::new();
    let mut path_base = 0u32;
    let mut is_split = false;

    // args[4]=leg_out, args[5]=leg_back. Parallel sub-routes share hop indices;
    // back continues after the longest out path (serial RT depth).
    for route_arg in &args.args[4..6] {
        let Some(sub_routes) = scval_vec(route_arg) else {
            continue;
        };
        if sub_routes.len() > 1 {
            is_split = true;
        }
        let mut max_depth = 0u32;
        for sub_route in &sub_routes {
            let Some(map) = scval_map(sub_route) else {
                continue;
            };
            let route_amount = get_map_i128(map, "amount_in");
            let Some(steps) = get_map_field(map, "steps").and_then(scval_vec) else {
                continue;
            };
            for (hop, step) in steps.iter().enumerate() {
                if let Some(step_map) = scval_map(step) {
                    legs.push(parse_step(step_map, path_base + hop as u32, route_amount)?);
                }
            }
            max_depth = max_depth.max(steps.len() as u32);
        }
        path_base = path_base.saturating_add(max_depth);
    }

    Ok(Some(ParsedInvocation {
        function_name: "round_trip_swap".into(),
        user_address,
        token_in: Some(base_token.clone()),
        token_out: Some(base_token),
        bridge_token: Some(bridge_token),
        amount_in,
        amount_out,
        is_split,
        legs,
    }))
}

fn parse_step(map: &xdr::ScMap, leg_index: u32, amount_in: Option<i128>) -> Result<ParsedLeg> {
    let pool_address = get_map_field(map, "dex_id")
        .map(scval_address_str)
        .transpose()?
        .unwrap_or_default();
    let dex_source = get_map_field(map, "dex_type")
        .map(parse_dex_type)
        .transpose()?
        .unwrap_or_else(|| "unknown".into());
    let token_in = get_map_field(map, "token_in").map(scval_address_str).transpose()?;
    let token_out = get_map_field(map, "token_out").map(scval_address_str).transpose()?;

    Ok(ParsedLeg {
        leg_index,
        dex_source,
        pool_address,
        token_in,
        token_out,
        amount_in,
        amount_out: None,
        amount_is_actual: false,
    })
}

fn parse_dex_type(val: &xdr::ScVal) -> Result<String> {
    match val {
        xdr::ScVal::Symbol(s) => Ok(symbol_dex_source(&s.to_string())),
        xdr::ScVal::Vec(Some(v)) if !v.is_empty() => match &v[0] {
            xdr::ScVal::Symbol(s) => Ok(symbol_dex_source(&s.to_string())),
            other => Err(anyhow!("unexpected dex_type vec element: {:?}", other)),
        },
        other => Err(anyhow!("unexpected dex_type: {:?}", other)),
    }
}

fn symbol_dex_source(sym: &str) -> String {
    match sym {
        "Aquarius" => "aquarius",
        "SoroswapPair" => "soroswap",
        "Phoenix" => "phoenix",
        "Sushi" => "sushi",
        "CometDex" => "comet",
        other => other,
    }
    .to_string()
}

fn parse_success_return_i128(result_meta_xdr: &str) -> Option<i128> {
    let bytes = BASE64.decode(result_meta_xdr.trim()).ok()?;
    let meta = xdr::TransactionMeta::from_xdr(&bytes, Limits::none()).ok()?;
    let return_value = match meta {
        xdr::TransactionMeta::V3(v3) => v3.soroban_meta?.return_value,
        xdr::TransactionMeta::V4(v4) => v4.soroban_meta?.return_value?,
        _ => return None,
    };
    scval_i128(&return_value)
}

fn contract_hash(contract: &str) -> Result<[u8; 32]> {
    Ok(Contract::from_string(contract)
        .with_context(|| format!("invalid contract id {}", contract))?
        .0)
}

fn scval_address_str(val: &xdr::ScVal) -> Result<String> {
    match val {
        xdr::ScVal::Address(addr) => match addr {
            xdr::ScAddress::Account(id) => {
                let pk = match &id.0 {
                    xdr::PublicKey::PublicKeyTypeEd25519(k) => k.0,
                };
                Ok(PublicKey(pk).to_string().to_string())
            }
            xdr::ScAddress::Contract(id) => Ok(Contract(id.0 .0).to_string().to_string()),
            other => Err(anyhow!("unsupported address type: {:?}", other)),
        },
        other => Err(anyhow!("expected address ScVal, got {:?}", other)),
    }
}

fn scval_i128(val: &xdr::ScVal) -> Option<i128> {
    match val {
        xdr::ScVal::I128(parts) => Some(((parts.hi as i128) << 64) | (parts.lo as i128)),
        _ => None,
    }
}

fn scval_vec(val: &xdr::ScVal) -> Option<Vec<xdr::ScVal>> {
    match val {
        xdr::ScVal::Vec(Some(v)) => Some(v.to_vec()),
        _ => None,
    }
}

fn scval_map(val: &xdr::ScVal) -> Option<&xdr::ScMap> {
    match val {
        xdr::ScVal::Map(Some(m)) => Some(m),
        _ => None,
    }
}

fn get_map_field<'a>(map: &'a xdr::ScMap, key: &str) -> Option<&'a xdr::ScVal> {
    map.0.iter().find_map(|entry| match &entry.key {
        xdr::ScVal::Symbol(s) if s.to_string() == key => Some(&entry.val),
        _ => None,
    })
}

fn get_map_i128(map: &xdr::ScMap, key: &str) -> Option<i128> {
    get_map_field(map, key).and_then(scval_i128)
}

#[cfg(test)]
mod tests {
    use {super::*, stellar_strkey::ed25519::PublicKey, stellar_xdr::curr::WriteXdr};

    const AGG: &str = "CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K";
    const USER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    const USDC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
    const POOL: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

    fn build_test_swap_envelope() -> String {
        let user_key = PublicKey::from_string(USER).unwrap();
        let agg_hash = contract_hash(AGG).unwrap();
        let usdc_hash = contract_hash(USDC).unwrap();
        let pool_hash = contract_hash(POOL).unwrap();

        let step = xdr::ScVal::Map(Some(xdr::ScMap(
            vec![
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("dex_id".try_into().unwrap()),
                    val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(pool_hash)))),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("dex_type".try_into().unwrap()),
                    val: xdr::ScVal::Symbol("SoroswapPair".try_into().unwrap()),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("in_idx".try_into().unwrap()),
                    val: xdr::ScVal::U32(0),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("out_idx".try_into().unwrap()),
                    val: xdr::ScVal::U32(1),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("token_in".try_into().unwrap()),
                    val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(usdc_hash)))),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("token_out".try_into().unwrap()),
                    val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(usdc_hash)))),
                },
            ]
            .try_into()
            .unwrap(),
        )));

        let sub_route = xdr::ScVal::Map(Some(xdr::ScMap(
            vec![
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("amount_in".try_into().unwrap()),
                    val: xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 1_000_000 }),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("steps".try_into().unwrap()),
                    val: xdr::ScVal::Vec(Some(xdr::ScVec(vec![step].try_into().unwrap()))),
                },
            ]
            .try_into()
            .unwrap(),
        )));

        let invoke_args = xdr::InvokeContractArgs {
            contract_address: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(agg_hash))),
            function_name: xdr::ScSymbol("swap".try_into().unwrap()),
            args: vec![
                xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
                    xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(user_key.0)),
                ))),
                xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(usdc_hash)))),
                xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(usdc_hash)))),
                xdr::ScVal::Vec(Some(xdr::ScVec(vec![sub_route].try_into().unwrap()))),
                xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 900_000 }),
            ]
            .try_into()
            .unwrap(),
        };

        let op = xdr::Operation {
            source_account: None,
            body: xdr::OperationBody::InvokeHostFunction(xdr::InvokeHostFunctionOp {
                host_function: xdr::HostFunction::InvokeContract(invoke_args),
                auth: xdr::VecM::default(),
            }),
        };

        let tx = xdr::Transaction {
            source_account: xdr::MuxedAccount::Ed25519(xdr::Uint256(user_key.0)),
            fee: 100_000,
            seq_num: xdr::SequenceNumber(1),
            cond: xdr::Preconditions::None,
            memo: xdr::Memo::None,
            operations: vec![op].try_into().unwrap(),
            ext: xdr::TransactionExt::V0,
        };

        xdr::TransactionEnvelope::Tx(xdr::TransactionV1Envelope {
            tx,
            signatures: xdr::VecM::default(),
        })
        .to_xdr_base64(Limits::none())
        .unwrap()
    }

    #[test]
    fn parses_synthetic_swap_envelope() {
        let env = build_test_swap_envelope();
        let parsed = parse_envelope(&env, AGG, None)
            .unwrap()
            .expect("expected swap invocation");
        assert_eq!(parsed.function_name, "swap");
        assert_eq!(parsed.user_address, USER);
        assert!(!parsed.is_split);
        assert_eq!(parsed.amount_in, 1_000_000);
        assert_eq!(parsed.legs.len(), 1);
        assert_eq!(parsed.legs[0].dex_source, "soroswap");
    }

    /// Optional mainnet fixture when present (see `tests/fixtures/README.md`).
    #[test]
    fn parses_mainnet_swap_fixture_if_present() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/swap_envelope.b64");
        if !path.exists() {
            return;
        }
        let env = std::fs::read_to_string(path).unwrap();
        let parsed = parse_envelope(env.trim(), AGG, None)
            .unwrap()
            .expect("fixture should decode to swap");
        assert_eq!(parsed.function_name, "swap");
    }

    /// Vault `execute_round_trip` wraps aggregator `round_trip_swap` in auth.
    /// Split 2+2 with 2 hops each must report serial depth 4 (not 8).
    #[test]
    fn parses_vault_split_round_trip_serial_hops() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vault_round_trip_split.b64");
        if !path.exists() {
            return;
        }
        let env = std::fs::read_to_string(path).unwrap();
        let parsed = parse_envelope(env.trim(), AGG, None)
            .unwrap()
            .expect("vault auth should expose round_trip_swap");
        assert_eq!(parsed.function_name, "round_trip_swap");
        assert!(parsed.is_split);
        let max_idx = parsed.legs.iter().map(|l| l.leg_index).max().unwrap();
        assert_eq!(max_idx + 1, 4, "serial hop depth should be 4 for 2hop+2hop RT");
        // Parallel paths share indices 0..3 → more than 4 leg rows.
        assert!(parsed.legs.len() > 4);
    }
}
