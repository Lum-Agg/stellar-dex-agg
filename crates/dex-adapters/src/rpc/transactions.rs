//! Soroban RPC `getTransactions` helpers for contract-scoped tx indexing.

use {
    super::SorobanRpc,
    anyhow::{anyhow, Result},
    serde::Deserialize,
    serde_json::json,
    std::collections::HashSet,
};

pub const DEFAULT_TX_PAGE_LIMIT: u32 = 200;
/// RPC allows scanning at most ~10k ledgers per `getTransactions` request.
pub const MAX_LEDGER_SCAN_PER_REQUEST: u32 = 10_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransaction {
    pub status: String,
    pub tx_hash: String,
    pub envelope_xdr: String,
    #[serde(default)]
    pub result_xdr: Option<String>,
    pub ledger: u32,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct GetTransactionsPage {
    pub transactions: Vec<RpcTransaction>,
    pub latest_ledger: u32,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TransactionFilterSpec {
    pub contract_ids: Option<Vec<String>>,
}

impl SorobanRpc {
    /// Fetch transactions touching filtered contracts in `[start_ledger,
    /// end_ledger)`.
    pub async fn get_contract_transactions(
        &self,
        start_ledger: u32,
        end_ledger: Option<u32>,
        filters: &[TransactionFilterSpec],
        limit: u32,
    ) -> Result<Vec<RpcTransaction>> {
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
        const MAX_PAGES: usize = 64;
        for _ in 0..MAX_PAGES {
            let page = self
                .get_contract_transactions_page(start_ledger, end_ledger, filters, limit, cursor.as_deref())
                .await?;
            if page.transactions.is_empty() {
                break;
            }
            for tx in page.transactions {
                all.push(tx);
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

    async fn get_contract_transactions_page(
        &self,
        start_ledger: u32,
        end_ledger: Option<u32>,
        filters: &[TransactionFilterSpec],
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<GetTransactionsPage> {
        let rpc_filters: Vec<serde_json::Value> = filters
            .iter()
            .map(|filter| {
                let mut obj = json!({ "type": "contract" });
                if let Some(ids) = &filter.contract_ids {
                    obj["contractIds"] = json!(ids);
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
            "method": "getTransactions",
            "params": params
        });

        let resp = self.post_json_with_retry(body).await?;
        if let Some(error) = resp.get("error") {
            return Err(anyhow!("getTransactions RPC error: {}", error));
        }
        let result = resp
            .get("result")
            .ok_or_else(|| anyhow!("getTransactions missing result"))?;

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RawTx {
            status: String,
            tx_hash: String,
            envelope_xdr: String,
            result_xdr: Option<String>,
            ledger: u32,
            created_at: i64,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RawResult {
            transactions: Vec<RawTx>,
            latest_ledger: u32,
            cursor: Option<String>,
        }

        let parsed: RawResult = serde_json::from_value(result.clone())
            .map_err(|e| anyhow!("getTransactions decode error: {} body={}", e, result))?;

        Ok(GetTransactionsPage {
            transactions: parsed
                .transactions
                .into_iter()
                .map(|t| RpcTransaction {
                    status: t.status,
                    tx_hash: t.tx_hash,
                    envelope_xdr: t.envelope_xdr,
                    result_xdr: t.result_xdr,
                    ledger: t.ledger,
                    created_at: t.created_at,
                })
                .collect(),
            latest_ledger: parsed.latest_ledger,
            cursor: parsed.cursor,
        })
    }
}
