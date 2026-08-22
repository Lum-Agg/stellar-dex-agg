//! Parse order-escrow lifecycle events for the analytics indexer.

use {
    crate::store::IndexStore,
    anyhow::{anyhow, Context, Result},
    base64::{engine::general_purpose::STANDARD as BASE64, Engine as _},
    chrono::DateTime,
    dex_adapters::rpc::events::ContractEvent,
    stellar_strkey::{ed25519::PublicKey, Contract},
    stellar_xdr::curr::{self as xdr, Limits, ReadXdr},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedOrderEvent {
    Created {
        escrow_contract: String,
        order_id: u64,
        owner: String,
        token_in: String,
        token_out: String,
        amount_in: String,
        limit_out_per_in_e7: String,
        expires_ledger: u32,
        ledger: u32,
        updated_at: i64,
    },
    Filled {
        escrow_contract: String,
        order_id: u64,
        amount_in_remaining: String,
        ledger: u32,
        updated_at: i64,
    },
    Cancelled {
        escrow_contract: String,
        order_id: u64,
        ledger: u32,
        updated_at: i64,
    },
    Expired {
        escrow_contract: String,
        order_id: u64,
        ledger: u32,
        updated_at: i64,
    },
    DcaCreated {
        escrow_contract: String,
        order_id: u64,
        owner: String,
        token_in: String,
        token_out: String,
        amount_in: String,
        chunk_amount: String,
        interval_ledgers: u32,
        next_executable_ledger: u32,
        min_out_per_in_e7: String,
        expires_ledger: u32,
        ledger: u32,
        updated_at: i64,
    },
    DcaFilled {
        escrow_contract: String,
        order_id: u64,
        amount_in_remaining: String,
        next_executable_ledger: u32,
        ledger: u32,
        updated_at: i64,
    },
    DcaCancelled {
        escrow_contract: String,
        order_id: u64,
        ledger: u32,
        updated_at: i64,
    },
    DcaExpired {
        escrow_contract: String,
        order_id: u64,
        ledger: u32,
        updated_at: i64,
    },
}

pub fn parse_escrow_order_event(event: &ContractEvent) -> Result<Option<ParsedOrderEvent>> {
    if event.event_type != "contract" || event.in_successful_contract_call == Some(false) {
        return Ok(None);
    }
    let Some((kind, order_id)) = event_topic(event)? else {
        return Ok(None);
    };
    let fields = event_data_fields(event)?;
    let updated_at = ledger_closed_at_to_unix(&event.ledger_closed_at, event.ledger);

    match kind.as_str() {
        "order_created" => {
            require_fields(&fields, 6, &kind)?;
            Ok(Some(ParsedOrderEvent::Created {
                escrow_contract: event.contract_id.clone(),
                order_id,
                owner: scval_address(&fields[0])?,
                token_in: scval_address(&fields[1])?,
                token_out: scval_address(&fields[2])?,
                amount_in: amount_to_string(scval_i128(&fields[3])?),
                limit_out_per_in_e7: amount_to_string(scval_i128(&fields[4])?),
                expires_ledger: scval_u32(&fields[5])?,
                ledger: event.ledger,
                updated_at,
            }))
        }
        "order_filled" => {
            require_fields(&fields, 4, &kind)?;
            Ok(Some(ParsedOrderEvent::Filled {
                escrow_contract: event.contract_id.clone(),
                order_id,
                amount_in_remaining: amount_to_string(scval_i128(&fields[3])?),
                ledger: event.ledger,
                updated_at,
            }))
        }
        "order_cancelled" => {
            require_fields(&fields, 2, &kind)?;
            Ok(Some(ParsedOrderEvent::Cancelled {
                escrow_contract: event.contract_id.clone(),
                order_id,
                ledger: event.ledger,
                updated_at,
            }))
        }
        "order_expired" => {
            require_fields(&fields, 2, &kind)?;
            Ok(Some(ParsedOrderEvent::Expired {
                escrow_contract: event.contract_id.clone(),
                order_id,
                ledger: event.ledger,
                updated_at,
            }))
        }
        "dca_created" => {
            require_fields(&fields, 9, &kind)?;
            Ok(Some(ParsedOrderEvent::DcaCreated {
                escrow_contract: event.contract_id.clone(),
                order_id,
                owner: scval_address(&fields[0])?,
                token_in: scval_address(&fields[1])?,
                token_out: scval_address(&fields[2])?,
                amount_in: amount_to_string(scval_i128(&fields[3])?),
                chunk_amount: amount_to_string(scval_i128(&fields[4])?),
                interval_ledgers: scval_u32(&fields[5])?,
                next_executable_ledger: scval_u32(&fields[6])?,
                min_out_per_in_e7: amount_to_string(scval_i128(&fields[7])?),
                expires_ledger: scval_u32(&fields[8])?,
                ledger: event.ledger,
                updated_at,
            }))
        }
        "dca_filled" => {
            require_fields(&fields, 5, &kind)?;
            Ok(Some(ParsedOrderEvent::DcaFilled {
                escrow_contract: event.contract_id.clone(),
                order_id,
                amount_in_remaining: amount_to_string(scval_i128(&fields[3])?),
                next_executable_ledger: scval_u32(&fields[4])?,
                ledger: event.ledger,
                updated_at,
            }))
        }
        "dca_cancelled" => Ok(Some(ParsedOrderEvent::DcaCancelled {
            escrow_contract: event.contract_id.clone(),
            order_id,
            ledger: event.ledger,
            updated_at,
        })),
        "dca_expired" => Ok(Some(ParsedOrderEvent::DcaExpired {
            escrow_contract: event.contract_id.clone(),
            order_id,
            ledger: event.ledger,
            updated_at,
        })),
        _ => Ok(None),
    }
}

/// Returns `true` when the event was applied, `false` when skipped because the
/// order is missing or already terminal.
pub fn apply_parsed_order_event(store: &IndexStore, event: &ParsedOrderEvent) -> Result<bool> {
    match event {
        ParsedOrderEvent::Created {
            escrow_contract,
            order_id,
            owner,
            token_in,
            token_out,
            amount_in,
            limit_out_per_in_e7,
            expires_ledger,
            ledger,
            updated_at,
        } => {
            store.upsert_created_for(
                escrow_contract,
                *order_id as i64,
                owner,
                token_in,
                token_out,
                amount_in,
                amount_in,
                limit_out_per_in_e7,
                *expires_ledger,
                *ledger,
                *ledger,
                *updated_at,
                *updated_at,
            )?;
            Ok(true)
        }
        ParsedOrderEvent::Filled {
            escrow_contract,
            order_id,
            amount_in_remaining,
            ledger,
            updated_at,
        } => store.apply_filled_for(
            escrow_contract,
            *order_id as i64,
            amount_in_remaining,
            *ledger,
            *updated_at,
        ),
        ParsedOrderEvent::Cancelled {
            escrow_contract,
            order_id,
            ledger,
            updated_at,
        } => store.apply_closed_for(escrow_contract, *order_id as i64, "cancelled", *ledger, *updated_at),
        ParsedOrderEvent::Expired {
            escrow_contract,
            order_id,
            ledger,
            updated_at,
        } => store.apply_closed_for(escrow_contract, *order_id as i64, "expired", *ledger, *updated_at),
        ParsedOrderEvent::DcaCreated {
            escrow_contract,
            order_id,
            owner,
            token_in,
            token_out,
            amount_in,
            chunk_amount,
            interval_ledgers,
            next_executable_ledger,
            min_out_per_in_e7,
            expires_ledger,
            ledger,
            updated_at,
        } => {
            store.upsert_dca_created_for(
                escrow_contract,
                *order_id as i64,
                owner,
                token_in,
                token_out,
                amount_in,
                chunk_amount,
                *interval_ledgers,
                *next_executable_ledger,
                min_out_per_in_e7,
                *expires_ledger,
                *ledger,
                *updated_at,
            )?;
            Ok(true)
        }
        ParsedOrderEvent::DcaFilled {
            escrow_contract,
            order_id,
            amount_in_remaining,
            next_executable_ledger,
            ledger,
            updated_at,
        } => store.apply_dca_filled_for(
            escrow_contract,
            *order_id as i64,
            amount_in_remaining,
            *next_executable_ledger,
            *ledger,
            *updated_at,
        ),
        ParsedOrderEvent::DcaCancelled {
            escrow_contract,
            order_id,
            ledger,
            updated_at,
        } => store.apply_dca_closed_for(escrow_contract, *order_id as i64, "cancelled", *ledger, *updated_at),
        ParsedOrderEvent::DcaExpired {
            escrow_contract,
            order_id,
            ledger,
            updated_at,
        } => store.apply_dca_closed_for(escrow_contract, *order_id as i64, "expired", *ledger, *updated_at),
    }
}

fn order_event_id(event: &ParsedOrderEvent) -> u64 {
    match event {
        ParsedOrderEvent::Created { order_id, .. } |
        ParsedOrderEvent::Filled { order_id, .. } |
        ParsedOrderEvent::Cancelled { order_id, .. } |
        ParsedOrderEvent::Expired { order_id, .. } => *order_id,
        ParsedOrderEvent::DcaCreated { order_id, .. } |
        ParsedOrderEvent::DcaFilled { order_id, .. } |
        ParsedOrderEvent::DcaCancelled { order_id, .. } |
        ParsedOrderEvent::DcaExpired { order_id, .. } => *order_id,
    }
}

fn order_event_kind(event: &ParsedOrderEvent) -> &'static str {
    match event {
        ParsedOrderEvent::Created { .. } => "order_created",
        ParsedOrderEvent::Filled { .. } => "order_filled",
        ParsedOrderEvent::Cancelled { .. } => "order_cancelled",
        ParsedOrderEvent::Expired { .. } => "order_expired",
        ParsedOrderEvent::DcaCreated { .. } => "dca_created",
        ParsedOrderEvent::DcaFilled { .. } => "dca_filled",
        ParsedOrderEvent::DcaCancelled { .. } => "dca_cancelled",
        ParsedOrderEvent::DcaExpired { .. } => "dca_expired",
    }
}

/// Apply escrow events, warning and continuing on parse skips and missing-order
/// no-ops.
pub fn ingest_escrow_order_events(store: &IndexStore, events: &[ContractEvent]) -> Result<u64> {
    let mut applied = 0u64;
    for event in events {
        let parsed = match parse_escrow_order_event(event) {
            Ok(Some(parsed)) => parsed,
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!(
                    event_id = %event.id,
                    tx = %event.tx_hash,
                    %error,
                    "failed to parse escrow order event"
                );
                continue;
            }
        };

        match apply_parsed_order_event(store, &parsed)? {
            true => applied += 1,
            false => {
                tracing::warn!(
                    order_id = order_event_id(&parsed),
                    kind = order_event_kind(&parsed),
                    event_id = %event.id,
                    tx = %event.tx_hash,
                    "skipped escrow order event: order not found or already terminal"
                );
            }
        }
    }
    Ok(applied)
}

fn amount_to_string(value: i128) -> String {
    if value == 0 {
        "0".to_string()
    } else {
        value.to_string()
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

fn ledger_closed_at_to_unix(closed_at: &Option<String>, ledger: u32) -> i64 {
    if let Some(ts) = closed_at {
        if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
            return dt.timestamp();
        }
    }
    ledger as i64 * 5
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        base64::{engine::general_purpose::STANDARD as BASE64, Engine},
        stellar_xdr::curr::{self as xdr, Limits, WriteXdr},
        tempfile::tempdir,
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
            ledger_closed_at: Some("2026-01-15T12:00:00Z".into()),
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
    fn parse_order_created() {
        let raw = event(
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
        let owner = Contract([1; 32]).to_string().to_string();
        let token_in = Contract([2; 32]).to_string().to_string();
        let token_out = Contract([3; 32]).to_string().to_string();
        let parsed = parse_escrow_order_event(&raw).unwrap().unwrap();
        assert_eq!(
            parsed,
            ParsedOrderEvent::Created {
                escrow_contract: "CESCROW".into(),
                order_id: 7,
                owner: owner.clone(),
                token_in: token_in.clone(),
                token_out: token_out.clone(),
                amount_in: "500".into(),
                limit_out_per_in_e7: "20000000".into(),
                expires_ledger: 999,
                ledger: 123,
                updated_at: 1_768_478_400,
            }
        );
    }

    #[test]
    fn parse_order_filled_uses_remaining_field() {
        let raw = event(
            "order_filled",
            7,
            vec![contract_address(1), i128_value(200), i128_value(410), i128_value(300)],
        );
        let parsed = parse_escrow_order_event(&raw).unwrap().unwrap();
        assert_eq!(
            parsed,
            ParsedOrderEvent::Filled {
                escrow_contract: "CESCROW".into(),
                order_id: 7,
                amount_in_remaining: "300".into(),
                ledger: 123,
                updated_at: 1_768_478_400,
            }
        );
    }

    #[test]
    fn parse_order_filled_zero_remaining() {
        let raw = event(
            "order_filled",
            8,
            vec![contract_address(1), i128_value(500), i128_value(1000), i128_value(0)],
        );
        let parsed = parse_escrow_order_event(&raw).unwrap().unwrap();
        match parsed {
            ParsedOrderEvent::Filled {
                amount_in_remaining, ..
            } => assert_eq!(amount_in_remaining, "0"),
            other => panic!("expected filled, got {other:?}"),
        }
    }

    #[test]
    fn parse_order_cancelled_and_expired() {
        let cancelled = event("order_cancelled", 9, vec![contract_address(1), i128_value(100)]);
        assert!(matches!(
            parse_escrow_order_event(&cancelled).unwrap(),
            Some(ParsedOrderEvent::Cancelled { order_id: 9, .. })
        ));

        let expired = event("order_expired", 10, vec![contract_address(1), i128_value(50)]);
        assert!(matches!(
            parse_escrow_order_event(&expired).unwrap(),
            Some(ParsedOrderEvent::Expired { order_id: 10, .. })
        ));
    }

    #[test]
    fn skips_failed_contract_call() {
        let mut raw = event(
            "order_created",
            1,
            vec![
                contract_address(1),
                contract_address(2),
                contract_address(3),
                i128_value(100),
                i128_value(1),
                xdr::ScVal::U32(100),
            ],
        );
        raw.in_successful_contract_call = Some(false);
        assert!(parse_escrow_order_event(&raw).unwrap().is_none());
    }

    #[test]
    fn skips_unknown_event_kind() {
        let raw = event("order_unknown", 1, vec![contract_address(1)]);
        assert!(parse_escrow_order_event(&raw).unwrap().is_none());
    }

    #[test]
    fn apply_parsed_order_event_returns_false_for_missing_order() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();

        let filled = event(
            "order_filled",
            99,
            vec![contract_address(1), i128_value(100), i128_value(200), i128_value(50)],
        );
        let parsed = parse_escrow_order_event(&filled).unwrap().unwrap();
        assert!(!apply_parsed_order_event(&store, &parsed).unwrap());
    }

    #[test]
    fn ingest_escrow_order_events_propagates_store_errors() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();
        store.conn().execute("DROP TABLE limit_orders", []).unwrap();

        let created = event(
            "order_created",
            1,
            vec![
                contract_address(1),
                contract_address(2),
                contract_address(3),
                i128_value(1000),
                i128_value(2_500_000),
                xdr::ScVal::U32(500),
            ],
        );

        assert!(ingest_escrow_order_events(&store, &[created]).is_err());
    }

    #[test]
    fn ingest_escrow_order_events_skips_missing_and_malformed_without_error() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();

        let missing_fill = event(
            "order_filled",
            99,
            vec![contract_address(1), i128_value(100), i128_value(200), i128_value(50)],
        );
        let mut malformed = event("order_created", 1, vec![contract_address(1)]);
        malformed.value = None;

        let created = event(
            "order_created",
            1,
            vec![
                contract_address(1),
                contract_address(2),
                contract_address(3),
                i128_value(1000),
                i128_value(2_500_000),
                xdr::ScVal::U32(500),
            ],
        );
        let filled = event(
            "order_filled",
            1,
            vec![contract_address(1), i128_value(400), i128_value(800), i128_value(600)],
        );

        let applied = ingest_escrow_order_events(&store, &[missing_fill, malformed, created, filled]).unwrap();
        assert_eq!(applied, 2);

        let owner = Contract([1; 32]).to_string().to_string();
        let row = store.list_by_owner_for("CESCROW", &owner, None).unwrap().pop().unwrap();
        assert_eq!(row.order_id, 1);
        assert_eq!(row.amount_in_remaining, "600");
    }

    #[test]
    fn apply_parsed_events_updates_store() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();

        let created = event(
            "order_created",
            42,
            vec![
                contract_address(1),
                contract_address(2),
                contract_address(3),
                i128_value(1000),
                i128_value(2_500_000),
                xdr::ScVal::U32(500),
            ],
        );
        let owner = Contract([1; 32]).to_string().to_string();
        apply_parsed_order_event(&store, &parse_escrow_order_event(&created).unwrap().unwrap()).unwrap();

        let rows = store.list_by_owner_for("CESCROW", &owner, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].order_id, 42);
        assert_eq!(rows[0].amount_in_remaining, "1000");
        assert_eq!(rows[0].status, "open");

        let filled = event(
            "order_filled",
            42,
            vec![contract_address(1), i128_value(400), i128_value(800), i128_value(600)],
        );
        apply_parsed_order_event(&store, &parse_escrow_order_event(&filled).unwrap().unwrap()).unwrap();
        let row = store.list_by_owner_for("CESCROW", &owner, None).unwrap().pop().unwrap();
        assert_eq!(row.amount_in_remaining, "600");

        let cancelled = event("order_cancelled", 42, vec![contract_address(1), i128_value(600)]);
        apply_parsed_order_event(&store, &parse_escrow_order_event(&cancelled).unwrap().unwrap()).unwrap();
        let row = store
            .list_by_owner_for("CESCROW", &owner, Some("all"))
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(row.status, "cancelled");
        assert_eq!(row.amount_in_remaining, "0");
    }
}
