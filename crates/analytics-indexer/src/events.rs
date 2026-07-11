//! Build indexer records from Soroban contract events (`getEvents`).

use {
    crate::{
        parser::{ParsedInvocation, ParsedLeg},
        store::StoredInvocation,
    },
    anyhow::{anyhow, Context, Result},
    base64::{engine::general_purpose::STANDARD as BASE64, Engine as _},
    chrono::DateTime,
    dex_adapters::rpc::events::ContractEvent,
    std::collections::BTreeMap,
    stellar_strkey::{ed25519::PublicKey, Contract},
    stellar_xdr::curr::{self as xdr, Limits, ReadXdr},
};

pub fn build_invocations_from_events(events: &[ContractEvent]) -> Result<Vec<StoredInvocation>> {
    let mut by_tx: BTreeMap<String, TxEventBundle> = BTreeMap::new();

    for event in events {
        if event.event_type != "contract" {
            continue;
        }
        let Some(kind) = event_topic_kind(event)? else {
            continue;
        };

        let bundle = by_tx.entry(event.tx_hash.clone()).or_insert_with(|| TxEventBundle {
            ledger: event.ledger,
            ledger_closed_at: event.ledger_closed_at.clone(),
            in_successful_contract_call: event.in_successful_contract_call,
            summary: None,
            legs: Vec::new(),
        });

        bundle.ledger = event.ledger;
        if event.ledger_closed_at.is_some() {
            bundle.ledger_closed_at = event.ledger_closed_at.clone();
        }
        if event.in_successful_contract_call.is_some() {
            bundle.in_successful_contract_call = event.in_successful_contract_call;
        }

        match kind.as_str() {
            "swap" => {
                if bundle.summary.is_none() {
                    bundle.summary = Some(parse_swap_summary(event)?);
                }
            }
            "rt" => {
                if bundle.summary.is_none() {
                    bundle.summary = Some(parse_round_trip_summary(event)?);
                }
            }
            "leg" => bundle.legs.push(parse_leg(event)?),
            _ => {}
        }
    }

    let mut out = Vec::new();
    for (tx_hash, bundle) in by_tx {
        let Some(summary) = bundle.summary else {
            continue;
        };
        let created_at = ledger_closed_at_to_unix(&bundle.ledger_closed_at, bundle.ledger);
        let status = if bundle.in_successful_contract_call == Some(false) {
            "FAILED".to_string()
        } else {
            "SUCCESS".to_string()
        };

        out.push(StoredInvocation {
            tx_hash,
            ledger: bundle.ledger,
            created_at,
            status,
            parsed: ParsedInvocation {
                function_name: summary.function_name,
                user_address: summary.user_address,
                token_in: summary.token_in,
                token_out: summary.token_out,
                amount_in: summary.amount_in,
                amount_out: Some(summary.amount_out),
                is_split: summary.is_split,
                legs: bundle.legs,
            },
        });
    }

    Ok(out)
}

struct TxEventBundle {
    ledger: u32,
    ledger_closed_at: Option<String>,
    in_successful_contract_call: Option<bool>,
    summary: Option<SummaryParsed>,
    legs: Vec<ParsedLeg>,
}

struct SummaryParsed {
    function_name: String,
    user_address: String,
    token_in: Option<String>,
    token_out: Option<String>,
    amount_in: i128,
    amount_out: i128,
    is_split: bool,
}

fn parse_swap_summary(event: &ContractEvent) -> Result<SummaryParsed> {
    let fields = event_data_fields(event)?;
    if fields.len() < 6 {
        return Err(anyhow!("swap event expects 6 data fields, got {}", fields.len()));
    }
    let route_count = scval_u32(&fields[5]).unwrap_or(0);
    Ok(SummaryParsed {
        function_name: "swap".into(),
        user_address: scval_address_str(&fields[0])?,
        token_in: Some(scval_address_str(&fields[1])?),
        token_out: Some(scval_address_str(&fields[2])?),
        amount_in: scval_i128(&fields[3]).unwrap_or(0),
        amount_out: scval_i128(&fields[4]).unwrap_or(0),
        is_split: route_count > 1,
    })
}

fn parse_round_trip_summary(event: &ContractEvent) -> Result<SummaryParsed> {
    let fields = event_data_fields(event)?;
    if fields.len() < 6 {
        return Err(anyhow!("rt event expects 6 data fields, got {}", fields.len()));
    }
    let base = scval_address_str(&fields[1])?;
    Ok(SummaryParsed {
        function_name: "round_trip_swap".into(),
        user_address: scval_address_str(&fields[0])?,
        token_in: Some(base.clone()),
        token_out: Some(base),
        amount_in: scval_i128(&fields[3]).unwrap_or(0),
        amount_out: scval_i128(&fields[4]).unwrap_or(0),
        is_split: false,
    })
}

fn parse_leg(event: &ContractEvent) -> Result<ParsedLeg> {
    let fields = event_data_fields(event)?;
    if fields.len() < 4 {
        return Err(anyhow!("leg event expects 4 data fields, got {}", fields.len()));
    }
    Ok(ParsedLeg {
        leg_index: scval_u32(&fields[0]).unwrap_or(0),
        dex_source: dex_tag_to_source(scval_u32(&fields[1]).unwrap_or(99)),
        pool_address: scval_address_str(&fields[2])?,
        token_in: None,
        token_out: None,
        amount_in: scval_i128(&fields[3]),
    })
}

fn event_topic_kind(event: &ContractEvent) -> Result<Option<String>> {
    let Some(topics) = &event.topic else {
        return Ok(None);
    };
    let Some(first) = topics.first() else {
        return Ok(None);
    };
    let scval = decode_scval_b64(first)?;
    Ok(match scval {
        xdr::ScVal::Symbol(s) => Some(s.to_string()),
        _ => None,
    })
}

fn event_data_fields(event: &ContractEvent) -> Result<Vec<xdr::ScVal>> {
    let Some(value) = &event.value else {
        return Err(anyhow!("event missing value"));
    };
    let xdr_b64 = value_xdr_b64(value)?;
    let scval = decode_scval_b64(&xdr_b64)?;
    match scval {
        xdr::ScVal::Vec(Some(v)) => Ok(v.to_vec()),
        other => Err(anyhow!("expected event data vec, got {:?}", other)),
    }
}

fn value_xdr_b64(value: &serde_json::Value) -> Result<String> {
    if let Some(s) = value.as_str() {
        return Ok(s.to_string());
    }
    if let Some(xdr) = value.get("xdr").and_then(|v| v.as_str()) {
        return Ok(xdr.to_string());
    }
    Err(anyhow!("unsupported event value json shape"))
}

fn decode_scval_b64(b64: &str) -> Result<xdr::ScVal> {
    let bytes = BASE64.decode(b64.trim()).context("decode event xdr base64")?;
    xdr::ScVal::from_xdr(&bytes, Limits::none()).context("decode event ScVal")
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
            other => Err(anyhow!("unsupported address: {:?}", other)),
        },
        other => Err(anyhow!("expected address, got {:?}", other)),
    }
}

fn scval_i128(val: &xdr::ScVal) -> Option<i128> {
    match val {
        xdr::ScVal::I128(parts) => Some(((parts.hi as i128) << 64) | (parts.lo as i128)),
        xdr::ScVal::U32(v) => Some(*v as i128),
        xdr::ScVal::U64(v) => Some(*v as i128),
        _ => None,
    }
}

fn scval_u32(val: &xdr::ScVal) -> Option<u32> {
    match val {
        xdr::ScVal::U32(v) => Some(*v),
        xdr::ScVal::I128(parts) if parts.hi == 0 && parts.lo <= u32::MAX as u64 => Some(parts.lo as u32),
        _ => None,
    }
}

fn dex_tag_to_source(tag: u32) -> String {
    match tag {
        0 => "aquarius".to_string(),
        1 => "soroswap".to_string(),
        2 => "phoenix".to_string(),
        3 => "sushi".to_string(),
        4 => "comet".to_string(),
        other => format!("dex_{other}"),
    }
}

fn ledger_closed_at_to_unix(closed_at: &Option<String>, ledger: u32) -> i64 {
    if let Some(ts) = closed_at {
        if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
            return dt.timestamp();
        }
    }
    // Fallback: approximate from ledger sequence (5s/ledger) if RPC omits
    // timestamp.
    ledger as i64 * 5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dex_tags_map() {
        assert_eq!(dex_tag_to_source(1), "soroswap");
        assert_eq!(dex_tag_to_source(3), "sushi");
    }
}
