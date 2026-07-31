//! Parse order-escrow lifecycle events into order-book updates.

use {
    crate::book::{OpenOrder, OrderEvent, OrderKind},
    anyhow::{anyhow, Context, Result},
    base64::{engine::general_purpose::STANDARD as BASE64, Engine as _},
    dex_adapters::rpc::events::ContractEvent,
    stellar_strkey::{ed25519::PublicKey, Contract},
    stellar_xdr::curr::{self as xdr, Limits, ReadXdr},
};

pub fn parse_escrow_event(event: &ContractEvent) -> Result<Option<OrderEvent>> {
    if event.event_type != "contract" || event.in_successful_contract_call == Some(false) {
        return Ok(None);
    }
    let Some((kind, order_id)) = event_topic(event)? else {
        return Ok(None);
    };
    let fields = event_data_fields(event)?;

    match kind.as_str() {
        "order_created" => {
            require_fields(&fields, 6, &kind)?;
            Ok(Some(OrderEvent::Created(OpenOrder {
                kind: OrderKind::Limit,
                order_id,
                owner: scval_address(&fields[0])?,
                token_in: scval_address(&fields[1])?,
                token_out: scval_address(&fields[2])?,
                amount_in_remaining: scval_i128(&fields[3])?,
                limit_out_per_in_e7: scval_i128(&fields[4])?,
                expires_ledger: scval_u32(&fields[5])?,
                chunk_amount: None,
                next_executable_ledger: None,
            })))
        }
        "order_filled" => {
            require_fields(&fields, 4, &kind)?;
            Ok(Some(OrderEvent::Filled {
                kind: OrderKind::Limit,
                order_id,
                amount_in_remaining: scval_i128(&fields[3])?,
                next_executable_ledger: None,
            }))
        }
        "order_cancelled" => {
            require_fields(&fields, 2, &kind)?;
            Ok(Some(OrderEvent::Cancelled {
                kind: OrderKind::Limit,
                order_id,
            }))
        }
        "order_expired" => {
            require_fields(&fields, 2, &kind)?;
            Ok(Some(OrderEvent::Expired {
                kind: OrderKind::Limit,
                order_id,
            }))
        }
        "dca_created" => {
            require_fields(&fields, 9, &kind)?;
            Ok(Some(OrderEvent::Created(OpenOrder {
                kind: OrderKind::Dca,
                order_id,
                owner: scval_address(&fields[0])?,
                token_in: scval_address(&fields[1])?,
                token_out: scval_address(&fields[2])?,
                amount_in_remaining: scval_i128(&fields[3])?,
                chunk_amount: Some(scval_i128(&fields[4])?),
                next_executable_ledger: Some(scval_u32(&fields[6])?),
                limit_out_per_in_e7: scval_i128(&fields[7])?,
                expires_ledger: scval_u32(&fields[8])?,
            })))
        }
        "dca_filled" => {
            require_fields(&fields, 5, &kind)?;
            Ok(Some(OrderEvent::Filled {
                kind: OrderKind::Dca,
                order_id,
                amount_in_remaining: scval_i128(&fields[3])?,
                next_executable_ledger: Some(scval_u32(&fields[4])?),
            }))
        }
        "dca_cancelled" => Ok(Some(OrderEvent::Cancelled {
            kind: OrderKind::Dca,
            order_id,
        })),
        "dca_expired" => Ok(Some(OrderEvent::Expired {
            kind: OrderKind::Dca,
            order_id,
        })),
        _ => Ok(None),
    }
}

fn event_topic(event: &ContractEvent) -> Result<Option<(String, u64)>> {
    let Some(topics) = &event.topic else {
        return Ok(None);
    };
    if topics.len() < 2 {
        return Ok(None);
    }
    let kind = match decode_scval(&topics[0])? {
        xdr::ScVal::Symbol(symbol) => symbol.to_string(),
        _ => return Ok(None),
    };
    let order_id = match decode_scval(&topics[1])? {
        xdr::ScVal::U64(value) => value,
        xdr::ScVal::U32(value) => value.into(),
        _ => return Ok(None),
    };
    Ok(Some((kind, order_id)))
}

fn event_data_fields(event: &ContractEvent) -> Result<Vec<xdr::ScVal>> {
    let value = event
        .value
        .as_ref()
        .and_then(|value| value.as_str().or_else(|| value.get("xdr").and_then(|xdr| xdr.as_str())))
        .ok_or_else(|| anyhow!("event missing value XDR"))?;
    match decode_scval(value)? {
        xdr::ScVal::Vec(Some(fields)) => Ok(fields.to_vec()),
        other => Err(anyhow!("expected event data vector, got {other:?}")),
    }
}

fn decode_scval(encoded: &str) -> Result<xdr::ScVal> {
    let bytes = BASE64.decode(encoded.trim()).context("decode event XDR base64")?;
    xdr::ScVal::from_xdr(&bytes, Limits::none()).context("decode event ScVal")
}

fn require_fields(fields: &[xdr::ScVal], count: usize, kind: &str) -> Result<()> {
    if fields.len() < count {
        return Err(anyhow!("{kind} expects {count} data fields, got {}", fields.len()));
    }
    Ok(())
}

fn scval_address(value: &xdr::ScVal) -> Result<String> {
    match value {
        xdr::ScVal::Address(xdr::ScAddress::Account(account)) => {
            let xdr::PublicKey::PublicKeyTypeEd25519(key) = &account.0;
            Ok(PublicKey(key.0).to_string().to_string())
        }
        xdr::ScVal::Address(xdr::ScAddress::Contract(id)) => Ok(Contract(id.0 .0).to_string().to_string()),
        other => Err(anyhow!("expected address, got {other:?}")),
    }
}

fn scval_i128(value: &xdr::ScVal) -> Result<i128> {
    match value {
        xdr::ScVal::I128(parts) => Ok(((parts.hi as i128) << 64) | parts.lo as i128),
        xdr::ScVal::U64(value) => Ok((*value).into()),
        xdr::ScVal::U32(value) => Ok((*value).into()),
        other => Err(anyhow!("expected integer amount, got {other:?}")),
    }
}

fn scval_u32(value: &xdr::ScVal) -> Result<u32> {
    match value {
        xdr::ScVal::U32(value) => Ok(*value),
        xdr::ScVal::U64(value) => (*value)
            .try_into()
            .map_err(|_| anyhow!("u64 ledger sequence does not fit u32: {value}")),
        other => Err(anyhow!("expected u32 ledger sequence, got {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use {
        super::parse_escrow_event,
        crate::book::{OpenOrderBook, OrderKind},
        base64::{engine::general_purpose::STANDARD as BASE64, Engine as _},
        dex_adapters::rpc::events::ContractEvent,
        stellar_xdr::curr::{self as xdr, Limits, WriteXdr},
    };

    fn encode(value: xdr::ScVal) -> String {
        BASE64.encode(value.to_xdr(Limits::none()).unwrap())
    }

    fn contract_address(byte: u8) -> xdr::ScVal {
        xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash([byte; 32]))))
    }

    fn i128_value(value: i128) -> xdr::ScVal {
        xdr::ScVal::I128(xdr::Int128Parts {
            hi: (value >> 64) as i64,
            lo: value as u64,
        })
    }

    fn event(kind: &str, order_id: u64, data: Vec<xdr::ScVal>) -> ContractEvent {
        ContractEvent {
            event_type: "contract".into(),
            ledger: 123,
            contract_id: "CESCROW".into(),
            id: format!("{kind}-{order_id}"),
            tx_hash: "tx".into(),
            ledger_closed_at: None,
            in_successful_contract_call: Some(true),
            topic: Some(vec![
                encode(xdr::ScVal::Symbol(kind.try_into().unwrap())),
                encode(xdr::ScVal::U64(order_id)),
            ]),
            value: Some(serde_json::Value::String(encode(xdr::ScVal::Vec(Some(
                data.try_into().unwrap(),
            ))))),
        }
    }

    #[test]
    fn parsed_lifecycle_events_update_open_order_book() {
        let created = event(
            "order_created",
            7,
            vec![
                contract_address(1),
                contract_address(2),
                contract_address(3),
                i128_value(500),
                i128_value(20_000_000),
                xdr::ScVal::U32(999),
            ],
        );
        let filled = event(
            "order_filled",
            7,
            vec![contract_address(1), i128_value(200), i128_value(410), i128_value(300)],
        );
        let cancelled = event("order_cancelled", 7, vec![contract_address(1), i128_value(300)]);

        let mut book = OpenOrderBook::default();
        book.apply(parse_escrow_event(&created).unwrap().unwrap());
        assert_eq!(book.get(OrderKind::Limit, 7).unwrap().amount_in_remaining, 500);
        book.apply(parse_escrow_event(&filled).unwrap().unwrap());
        assert_eq!(book.get(OrderKind::Limit, 7).unwrap().amount_in_remaining, 300);
        book.apply(parse_escrow_event(&cancelled).unwrap().unwrap());
        assert!(book.get(OrderKind::Limit, 7).is_none());
    }
}
