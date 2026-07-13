//! Soroban RPC `getLatestLedger` / `getEvents` helpers for ledger-driven
//! indexing.

use {
    super::SorobanRpc,
    anyhow::{anyhow, Result},
    serde::Deserialize,
    serde_json::json,
    std::collections::HashSet,
};

pub const DEFAULT_EVENTS_PAGE_LIMIT: u32 = 10_000;
/// RPC allows scanning at most ~10k ledgers per `getEvents` request.
pub const MAX_LEDGER_SCAN_PER_REQUEST: u32 = 10_000;

#[derive(Debug, Clone, Deserialize)]
pub struct LatestLedger {
    pub sequence: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContractEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub ledger: u32,
    #[serde(rename = "contractId")]
    pub contract_id: String,
    pub id: String,
    #[serde(rename = "txHash")]
    pub tx_hash: String,
    #[serde(rename = "ledgerClosedAt", default)]
    pub ledger_closed_at: Option<String>,
    #[serde(rename = "inSuccessfulContractCall", default)]
    pub in_successful_contract_call: Option<bool>,
    /// Base64-encoded event payload XDR (ScVal), or `{"xdr":"..."}` from some
    /// RPCs.
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    #[serde(default)]
    pub topic: Option<Vec<String>>,
}

/// Event `value` may be a bare XDR base64 string or `{"xdr":"..."}`.
pub fn event_value_xdr(value: Option<&serde_json::Value>) -> Option<&str> {
    value.and_then(|v| v.as_str().or_else(|| v.get("xdr").and_then(|x| x.as_str())))
}

#[derive(Debug, Clone)]
pub struct GetEventsPage {
    pub events: Vec<ContractEvent>,
    pub latest_ledger: u32,
    /// Earliest ledger retained by this RPC node's event store.
    pub oldest_ledger: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EventFilterSpec {
    pub contract_ids: Option<Vec<String>>,
    /// Each inner vec is a topic matcher row (use `"*"` / `"**"` per RPC docs).
    pub topics: Option<Vec<Vec<String>>>,
}

impl SorobanRpc {
    pub async fn get_latest_ledger(&self) -> Result<LatestLedger> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestLedger",
            "params": {}
        });
        let resp = self.post_json_with_retry(body).await?;
        if let Some(error) = resp.get("error") {
            return Err(anyhow!("getLatestLedger RPC error: {}", error));
        }
        let sequence = resp
            .get("result")
            .and_then(|r| r.get("sequence"))
            .and_then(|s| s.as_u64())
            .ok_or_else(|| anyhow!("getLatestLedger missing sequence"))?;
        Ok(LatestLedger {
            sequence: sequence as u32,
        })
    }

    /// Probe `getEvents` at `latest` to learn RPC event retention bounds.
    pub async fn get_events_ledger_bounds(&self, contract_id: &str) -> Result<(u32, u32)> {
        let latest = self.get_latest_ledger().await?.sequence;
        let filters = [EventFilterSpec {
            contract_ids: Some(vec![contract_id.to_string()]),
            topics: None,
        }];
        let page = self
            .get_contract_events_page(latest, Some(latest), &filters, 1, None)
            .await?;
        let oldest = page.oldest_ledger.unwrap_or(latest);
        Ok((oldest, page.latest_ledger))
    }

    /// Fetch contract events in `[start_ledger, end_ledger)` with pagination.
    pub async fn get_contract_events(
        &self,
        start_ledger: u32,
        end_ledger: Option<u32>,
        filters: &[EventFilterSpec],
        limit: u32,
    ) -> Result<Vec<ContractEvent>> {
        if start_ledger == 0 {
            return Err(anyhow!("start_ledger must be > 0"));
        }
        if let Some(end) = end_ledger {
            if end <= start_ledger {
                return Ok(Vec::new());
            }
            if end.saturating_sub(start_ledger) > MAX_LEDGER_SCAN_PER_REQUEST {
                return Err(anyhow!(
                    "ledger span {} exceeds RPC max {}",
                    end - start_ledger,
                    MAX_LEDGER_SCAN_PER_REQUEST
                ));
            }
        }

        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        let mut seen_cursors: HashSet<String> = HashSet::new();
        const MAX_PAGES: usize = 32;
        for _ in 0..MAX_PAGES {
            let page = self
                .get_contract_events_page(start_ledger, end_ledger, filters, limit, cursor.as_deref())
                .await?;
            if page.events.is_empty() {
                break;
            }
            for event in page.events {
                all.push(event);
            }
            match page.cursor {
                Some(next) if !next.is_empty() && seen_cursors.insert(next.clone()) => {
                    cursor = Some(next);
                }
                _ => break,
            }
        }
        Ok(all)
    }

    async fn get_contract_events_page(
        &self,
        start_ledger: u32,
        end_ledger: Option<u32>,
        filters: &[EventFilterSpec],
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<GetEventsPage> {
        let rpc_filters: Vec<serde_json::Value> = filters
            .iter()
            .map(|filter| {
                let mut obj = json!({ "type": "contract" });
                if let Some(ids) = &filter.contract_ids {
                    obj["contractIds"] = json!(ids);
                }
                if let Some(topics) = &filter.topics {
                    obj["topics"] = json!(topics);
                }
                obj
            })
            .collect();

        let mut params = json!({
            "filters": rpc_filters,
            "pagination": { "limit": limit }
        });
        if let Some(cursor) = cursor {
            params["pagination"]["cursor"] = json!(cursor);
        } else {
            params["startLedger"] = json!(start_ledger);
            if let Some(end) = end_ledger {
                params["endLedger"] = json!(end);
            }
        }

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getEvents",
            "params": params
        });

        let resp = self.post_json_with_retry(body).await?;
        if let Some(error) = resp.get("error") {
            return Err(anyhow!("getEvents RPC error: {}", error));
        }
        let result = resp.get("result").ok_or_else(|| anyhow!("getEvents missing result"))?;

        #[derive(Deserialize)]
        struct RawEvent {
            #[serde(rename = "type")]
            event_type: String,
            ledger: u32,
            #[serde(rename = "contractId")]
            contract_id: String,
            id: String,
            #[serde(rename = "txHash")]
            tx_hash: String,
            #[serde(rename = "ledgerClosedAt", default)]
            ledger_closed_at: Option<String>,
            #[serde(rename = "inSuccessfulContractCall", default)]
            in_successful_contract_call: Option<bool>,
            #[serde(default)]
            value: Option<serde_json::Value>,
            #[serde(default)]
            topic: Option<Vec<String>>,
        }

        #[derive(Deserialize)]
        struct RawResult {
            events: Vec<RawEvent>,
            #[serde(rename = "latestLedger")]
            latest_ledger: u32,
            #[serde(rename = "oldestLedger")]
            oldest_ledger: Option<u32>,
            cursor: Option<String>,
        }

        let parsed: RawResult = serde_json::from_value(result.clone())
            .map_err(|e| anyhow!("getEvents decode error: {} body={}", e, result))?;

        Ok(GetEventsPage {
            events: parsed
                .events
                .into_iter()
                .map(|e| ContractEvent {
                    event_type: e.event_type,
                    ledger: e.ledger,
                    contract_id: e.contract_id,
                    id: e.id,
                    tx_hash: e.tx_hash,
                    ledger_closed_at: e.ledger_closed_at,
                    in_successful_contract_call: e.in_successful_contract_call,
                    value: e.value,
                    topic: e.topic,
                })
                .collect(),
            latest_ledger: parsed.latest_ledger,
            oldest_ledger: parsed.oldest_ledger,
            cursor: parsed.cursor,
        })
    }
}
