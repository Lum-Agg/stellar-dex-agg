//! Dump getEvents output per ledger to JSONL files.
//!
//!   RPC_URL=http://127.0.0.1:8003 DUMP_DIR=./ledger-events-dump DUMP_LEDGERS=5 \
//!     cargo run -p dex-adapters --release --bin dump-ledger-events

use {
    base64::Engine,
    dex_adapters::{
        aquarius::AQUARIUS_ROUTER,
        pool_index::{touched_pools_from_events, KnownPoolIndex},
        router_events::pools_from_router_event,
        rpc::{
            events::{event_value_xdr, ContractEvent},
            SorobanRpc,
        },
        soroswap::SOROSWAP_ROUTER,
        utils::is_contract_address,
    },
    market_snapshot::MarketSnapshot,
    serde::Serialize,
    std::{fs, path::PathBuf},
    stellar_xdr::curr::{Limits, ReadXdr, ScVal},
};

#[derive(Serialize)]
struct DumpEvent<'a> {
    id: &'a str,
    event_type: &'a str,
    ledger: u32,
    contract_id: &'a str,
    tx_hash: &'a str,
    topic: Option<&'a [String]>,
    topic_decoded: Vec<String>,
    value: Option<&'a str>,
    annotations: EventAnnotations,
}

#[derive(Serialize)]
struct EventAnnotations {
    is_known_pool_contract: bool,
    is_aquarius_router: bool,
    is_soroswap_router: bool,
    router_parsed_pools: Vec<String>,
    in_touched_set: bool,
}

#[derive(Serialize)]
struct LedgerSummary {
    ledger: u32,
    event_count: usize,
    contract_event_count: usize,
    known_pool_contract_events: usize,
    aquarius_router_events: usize,
    soroswap_router_events: usize,
    router_parse_ok: usize,
    router_parse_fail: usize,
    touched_pools: Vec<TouchedPoolLine>,
    output_file: String,
}

#[derive(Serialize)]
struct TouchedPoolLine {
    source: String,
    pool_address: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let rpc_url = std::env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8003".into());
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://:REDISzlg153@127.0.0.1:6379/".into());
    let dump_dir = PathBuf::from(std::env::var("DUMP_DIR").unwrap_or_else(|_| "ledger-events-dump".into()));
    let ledgers: u32 = std::env::var("DUMP_LEDGERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let only_with_dex = std::env::var("DUMP_ONLY_DEX")
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    fs::create_dir_all(&dump_dir)?;

    let snap = load_snapshot_from_redis(&redis_url).await?;
    let index = KnownPoolIndex::rebuild(&snap.sources, &snap.clmm_pool_refs);

    let rpc = SorobanRpc::new(&rpc_url, "Public Global Stellar Network ; September 2015");
    let latest = rpc.get_latest_ledger().await?.sequence;
    let start = latest.saturating_sub(ledgers - 1);

    let mut summaries = Vec::new();
    for ledger in start..=latest {
        let events = fetch_ledger_events(&rpc, ledger).await?;
        let touched = touched_pools_from_events(&events, &index);
        let touched_addrs: std::collections::HashSet<String> = touched.iter().map(|p| p.pool_address.clone()).collect();

        let contract_events: Vec<_> = events.iter().filter(|e| e.event_type == "contract").collect();

        let mut router_parse_ok = 0usize;
        let mut router_parse_fail = 0usize;
        let mut aquarius_router_events = 0usize;
        let mut soroswap_router_events = 0usize;
        let mut known_pool_contract_events = 0usize;

        for e in &contract_events {
            if index.lookup_contract(&e.contract_id).is_some() {
                known_pool_contract_events += 1;
            }
            if e.contract_id == AQUARIUS_ROUTER {
                aquarius_router_events += 1;
            }
            if e.contract_id == SOROSWAP_ROUTER {
                soroswap_router_events += 1;
            }
            if e.contract_id == AQUARIUS_ROUTER || e.contract_id == SOROSWAP_ROUTER {
                let parsed =
                    pools_from_router_event(&e.contract_id, e.topic.as_deref(), event_value_xdr(e.value.as_ref()));
                if parsed.is_empty() {
                    router_parse_fail += 1;
                } else {
                    router_parse_ok += parsed.len();
                }
            }
        }

        if only_with_dex && touched.is_empty() {
            continue;
        }

        let filename = format!("ledger-{ledger}.jsonl");
        let path = dump_dir.join(&filename);
        let mut out = fs::File::create(&path)?;

        for e in &events {
            let line = DumpEvent {
                id: &e.id,
                event_type: &e.event_type,
                ledger: e.ledger,
                contract_id: &e.contract_id,
                tx_hash: &e.tx_hash,
                topic: e.topic.as_deref(),
                topic_decoded: decode_topics(e.topic.as_deref()),
                value: event_value_xdr(e.value.as_ref()),
                annotations: EventAnnotations {
                    is_known_pool_contract: index.lookup_contract(&e.contract_id).is_some(),
                    is_aquarius_router: e.contract_id == AQUARIUS_ROUTER,
                    is_soroswap_router: e.contract_id == SOROSWAP_ROUTER,
                    router_parsed_pools: pools_from_router_event(
                        &e.contract_id,
                        e.topic.as_deref(),
                        event_value_xdr(e.value.as_ref()),
                    ),
                    in_touched_set: index.lookup_contract(&e.contract_id).is_some()
                        || pools_from_router_event(
                            &e.contract_id,
                            e.topic.as_deref(),
                            event_value_xdr(e.value.as_ref()),
                        )
                        .into_iter()
                        .any(|addr| touched_addrs.contains(&addr)),
                },
            };
            use std::io::Write;
            writeln!(out, "{}", serde_json::to_string(&line)?)?;
        }

        let touched_pools: Vec<TouchedPoolLine> = touched
            .iter()
            .map(|p| TouchedPoolLine {
                source: p.source.clone(),
                pool_address: p.pool_address.clone(),
            })
            .collect();

        summaries.push(LedgerSummary {
            ledger,
            event_count: events.len(),
            contract_event_count: contract_events.len(),
            known_pool_contract_events,
            aquarius_router_events,
            soroswap_router_events,
            router_parse_ok,
            router_parse_fail,
            touched_pools,
            output_file: filename,
        });

        println!("ledger {ledger}: wrote {} events -> {}", events.len(), path.display());
    }

    let index_path = dump_dir.join("_index.json");
    fs::write(&index_path, serde_json::to_string_pretty(&summaries)?)?;
    println!("index -> {}", index_path.display());
    Ok(())
}

async fn fetch_ledger_events(rpc: &SorobanRpc, ledger: u32) -> anyhow::Result<Vec<ContractEvent>> {
    use dex_adapters::rpc::events::{EventFilterSpec, DEFAULT_EVENTS_PAGE_LIMIT};

    let filters = vec![EventFilterSpec {
        contract_ids: None,
        topics: Some(vec![vec!["**".to_string()]]),
    }];
    rpc.get_contract_events(ledger, Some(ledger + 1), &filters, DEFAULT_EVENTS_PAGE_LIMIT)
        .await
}

async fn load_snapshot_from_redis(redis_url: &str) -> anyhow::Result<MarketSnapshot> {
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_multiplexed_async_connection().await?;
    let current: String = redis::cmd("GET")
        .arg("lumagg:snapshot:current")
        .query_async(&mut conn)
        .await?;
    let raw: String = redis::cmd("GET")
        .arg(format!("lumagg:snapshot:data:{current}"))
        .query_async(&mut conn)
        .await?;
    Ok(serde_json::from_str(&raw)?)
}

fn decode_topics(topics: Option<&[String]>) -> Vec<String> {
    topics
        .unwrap_or(&[])
        .iter()
        .map(|raw| decode_scval_label(raw).unwrap_or_else(|| format!("b64:{raw}")))
        .collect()
}

fn decode_scval_label(raw: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(raw).ok()?;
    let scval = ScVal::from_xdr(&bytes, Limits::none()).ok()?;
    match scval {
        ScVal::Symbol(s) => Some(format!("sym:{}", s.to_string())),
        ScVal::String(s) => Some(format!("str:{}", s.to_string())),
        ScVal::Address(a) => {
            if let Ok(addr) = dex_adapters::rpc::scval_to_address(&ScVal::Address(a)) {
                if is_contract_address(&addr) {
                    Some(format!("addr:{addr}"))
                } else {
                    Some(format!("addr:{addr}"))
                }
            } else {
                Some("addr:?".into())
            }
        }
        ScVal::U32(v) => Some(format!("u32:{v}")),
        ScVal::I32(v) => Some(format!("i32:{v}")),
        ScVal::U64(v) => Some(format!("u64:{v}")),
        ScVal::I64(v) => Some(format!("i64:{v}")),
        _ => Some(format!("{:?}", std::mem::discriminant(&scval))),
    }
}
