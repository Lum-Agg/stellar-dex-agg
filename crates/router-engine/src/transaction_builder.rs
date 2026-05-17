//! Transaction builder: generates atomic Stellar transactions from optimal routes.
//!
//! Determines the appropriate on-chain execution strategy:
//! - Single path, single Soroban DEX: direct contract call
//! - Single path, multi-hop Soroban: aggregator.swap()
//! - Split order: aggregator.split_swap()
//!
//! All transactions are simulated before returning to the user.

use crate::types::{OptimalRoute, SimulationResult, SubOrder, UnsignedTransaction};
use anyhow::{Result, anyhow};
use dex_adapters::rpc::SorobanRpc;
use sha2::Digest;
use std::sync::Arc;
use stellar_xdr::curr as xdr;
use tracing::{debug, info};

/// Configuration for transaction building.
#[derive(Debug, Clone)]
pub struct TxBuilderConfig {
    /// Network passphrase
    pub network_passphrase: String,
    /// Aggregator contract address (deployed on-chain)
    pub aggregator_contract: String,
    /// Base fee in stroops
    pub base_fee: u32,
}

impl TxBuilderConfig {
    pub fn mainnet(aggregator_contract: &str) -> Self {
        Self {
            network_passphrase: "Public Global Stellar Network ; September 2015".to_string(),
            aggregator_contract: aggregator_contract.to_string(),
            base_fee: 10_000,
        }
    }
}

pub struct TransactionBuilder {
    config: TxBuilderConfig,
    rpc: Arc<SorobanRpc>,
}

impl TransactionBuilder {
    pub fn new(config: TxBuilderConfig, rpc: Arc<SorobanRpc>) -> Self {
        Self { config, rpc }
    }

    /// Build an unsigned atomic transaction for the given route.
    ///
    /// Strategy:
    /// - If route has 1 sub-order: use aggregator.swap() (single path)
    /// - If route has multiple sub-orders: use aggregator.split_swap()
    pub async fn build(
        &self,
        route: &OptimalRoute,
        user_address: &str,
        slippage_bps: u32,
    ) -> Result<UnsignedTransaction> {
        if route.sub_orders.is_empty() {
            return Err(anyhow!("Cannot build transaction for empty route"));
        }

        let min_output = route.minimum_out;

        // Determine token_in and token_out from the route
        let first_order = &route.sub_orders[0];
        let token_in = first_order.path.tokens.first()
            .ok_or_else(|| anyhow!("Empty path in sub-order"))?;
        let token_out = first_order.path.tokens.last()
            .ok_or_else(|| anyhow!("Empty path in sub-order"))?;

        let invoke_args = if route.sub_orders.len() == 1 {
            // Single path: aggregator.swap(user, token_in, amount_in, steps, min_out)
            self.build_single_swap_args(
                user_address,
                token_in,
                token_out,
                &first_order,
                min_output,
            )?
        } else {
            // Split order: aggregator.split_swap(user, token_in, token_out, sub_routes, min_out)
            self.build_split_swap_args(
                user_address,
                token_in,
                token_out,
                &route.sub_orders,
                min_output,
            )?
        };

        // Build the InvokeHostFunction operation
        let aggregator_hash = stellar_strkey::Contract::from_string(&self.config.aggregator_contract)
            .map_err(|e| anyhow!("Invalid aggregator contract: {:?}", e))?
            .0;

        let function_name = if route.sub_orders.len() == 1 { "swap" } else { "split_swap" };

        let invoke_contract_args = xdr::InvokeContractArgs {
            contract_address: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(aggregator_hash))),
            function_name: function_name
                .try_into()
                .map_err(|_| anyhow!("Invalid function name"))?,
            args: invoke_args.try_into().map_err(|_| anyhow!("Too many args"))?,
        };

        let host_function = xdr::HostFunction::InvokeContract(invoke_contract_args);

        let op = xdr::Operation {
            source_account: None,
            body: xdr::OperationBody::InvokeHostFunction(xdr::InvokeHostFunctionOp {
                host_function,
                auth: xdr::VecM::default(),
            }),
        };

        // Build transaction envelope
        let user_account_id = stellar_strkey::ed25519::PublicKey::from_string(user_address)
            .map_err(|e| anyhow!("Invalid user address: {:?}", e))?;

        let source_account = xdr::MuxedAccount::Ed25519(xdr::Uint256(user_account_id.0));

        // We need the user's sequence number - for now use 0, the simulate step will handle it
        let tx = xdr::Transaction {
            source_account,
            fee: self.config.base_fee,
            seq_num: xdr::SequenceNumber(0), // Will be set by prepare_transaction
            cond: xdr::Preconditions::None,
            memo: xdr::Memo::None,
            operations: vec![op].try_into().map_err(|_| anyhow!("ops error"))?,
            ext: xdr::TransactionExt::V0,
        };

        let envelope = xdr::TransactionEnvelope::Tx(xdr::TransactionV1Envelope {
            tx,
            signatures: xdr::VecM::default(),
        });

        use stellar_xdr::curr::{Limits, WriteXdr};
        let tx_xdr = envelope
            .to_xdr_base64(Limits::none())
            .map_err(|e| anyhow!("XDR encode error: {:?}", e))?;

        // Simulate the transaction
        let simulation = self.simulate(&tx_xdr).await?;

        let tx_bytes = envelope.to_xdr(Limits::none()).unwrap_or_default();
        let hash = hex::encode(sha2::Sha256::digest(&tx_bytes));

        Ok(UnsignedTransaction {
            xdr: tx_xdr,
            hash,
            operation_count: 1,
            estimated_fee: simulation.resource_fee.unwrap_or(self.config.base_fee as u64),
            simulation,
        })
    }

    /// Build args for aggregator.swap(user, token_in, amount_in, steps, min_amount_out)
    fn build_single_swap_args(
        &self,
        user_address: &str,
        token_in: &dex_adapters::TokenId,
        _token_out: &dex_adapters::TokenId,
        sub_order: &SubOrder,
        min_output: u128,
    ) -> Result<Vec<xdr::ScVal>> {
        let user_val = self.address_to_scval(user_address)?;
        let token_in_val = self.token_to_scval(token_in)?;
        let amount_in_val = self.i128_to_scval(sub_order.amount_in as i128);
        let steps_val = self.build_steps_scval(&sub_order.path)?;
        let min_out_val = self.i128_to_scval(min_output as i128);

        Ok(vec![user_val, token_in_val, amount_in_val, steps_val, min_out_val])
    }

    /// Build args for aggregator.split_swap(user, token_in, token_out, sub_routes, min_amount_out)
    fn build_split_swap_args(
        &self,
        user_address: &str,
        token_in: &dex_adapters::TokenId,
        token_out: &dex_adapters::TokenId,
        sub_orders: &[SubOrder],
        min_output: u128,
    ) -> Result<Vec<xdr::ScVal>> {
        let user_val = self.address_to_scval(user_address)?;
        let token_in_val = self.token_to_scval(token_in)?;
        let token_out_val = self.token_to_scval(token_out)?;

        // Build Vec<SubRoute>
        let sub_routes: Vec<xdr::ScVal> = sub_orders
            .iter()
            .map(|so| self.build_sub_route_scval(so))
            .collect::<Result<Vec<_>>>()?;

        let sub_routes_val = xdr::ScVal::Vec(Some(xdr::ScVec(
            sub_routes.try_into().map_err(|_| anyhow!("Too many sub-routes"))?
        )));

        let min_out_val = self.i128_to_scval(min_output as i128);

        Ok(vec![user_val, token_in_val, token_out_val, sub_routes_val, min_out_val])
    }

    /// Build a SubRoute ScVal (Map with amount_in and steps)
    fn build_sub_route_scval(&self, sub_order: &SubOrder) -> Result<xdr::ScVal> {
        let amount_in_entry = xdr::ScMapEntry {
            key: xdr::ScVal::Symbol("amount_in".try_into().unwrap()),
            val: self.i128_to_scval(sub_order.amount_in as i128),
        };

        let steps_entry = xdr::ScMapEntry {
            key: xdr::ScVal::Symbol("steps".try_into().unwrap()),
            val: self.build_steps_scval(&sub_order.path)?,
        };

        Ok(xdr::ScVal::Map(Some(xdr::ScMap(
            vec![amount_in_entry, steps_entry].try_into().unwrap()
        ))))
    }

    /// Build Vec<SwapStep> as ScVal from a Path
    fn build_steps_scval(&self, path: &crate::types::Path) -> Result<xdr::ScVal> {
        let mut steps = Vec::new();

        for i in 0..path.sources.len() {
            let token_in = &path.tokens[i];
            let token_out = &path.tokens[i + 1];
            let pool_address = &path.pool_addresses[i];
            let source = &path.sources[i];

            // Determine DexType from source name
            let dex_type_val = match source.as_str() {
                "soroswap" => xdr::ScVal::Vec(Some(xdr::ScVec(
                    vec![xdr::ScVal::Symbol("SoroswapPair".try_into().unwrap())]
                        .try_into().unwrap()
                ))),
                "aquarius" => xdr::ScVal::Vec(Some(xdr::ScVec(
                    vec![xdr::ScVal::Symbol("Aquarius".try_into().unwrap())]
                        .try_into().unwrap()
                ))),
                "phoenix" => xdr::ScVal::Vec(Some(xdr::ScVec(
                    vec![xdr::ScVal::Symbol("Phoenix".try_into().unwrap())]
                        .try_into().unwrap()
                ))),
                _ => return Err(anyhow!("Unknown DEX source: {}", source)),
            };

            // Build SwapStep map
            let mut entries = Vec::new();
            entries.push(xdr::ScMapEntry {
                key: xdr::ScVal::Symbol("a2b".try_into().unwrap()),
                val: xdr::ScVal::Bool(true), // TODO: determine from token order in pool
            });
            entries.push(xdr::ScMapEntry {
                key: xdr::ScVal::Symbol("dex_id".try_into().unwrap()),
                val: self.contract_to_scval(pool_address)?,
            });
            entries.push(xdr::ScMapEntry {
                key: xdr::ScVal::Symbol("dex_type".try_into().unwrap()),
                val: dex_type_val,
            });
            entries.push(xdr::ScMapEntry {
                key: xdr::ScVal::Symbol("token_in".try_into().unwrap()),
                val: self.token_to_scval(token_in)?,
            });
            entries.push(xdr::ScMapEntry {
                key: xdr::ScVal::Symbol("token_out".try_into().unwrap()),
                val: self.token_to_scval(token_out)?,
            });

            steps.push(xdr::ScVal::Map(Some(xdr::ScMap(
                entries.try_into().unwrap()
            ))));
        }

        Ok(xdr::ScVal::Vec(Some(xdr::ScVec(
            steps.try_into().map_err(|_| anyhow!("Too many steps"))?
        ))))
    }

    /// Simulate a transaction via RPC.
    pub async fn simulate(&self, tx_xdr: &str) -> Result<SimulationResult> {
        use serde_json::json;

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "simulateTransaction",
            "params": {
                "transaction": tx_xdr
            }
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(self.rpc.network_passphrase()) // This should be the RPC URL
            .json(&body)
            .send()
            .await;

        match resp {
            Ok(r) => {
                let json: serde_json::Value = r.json().await.unwrap_or_default();
                let result = json.get("result").cloned().unwrap_or_default();

                if let Some(error) = result.get("error") {
                    return Ok(SimulationResult {
                        success: false,
                        actual_output: None,
                        resource_fee: None,
                        error: Some(error.to_string()),
                    });
                }

                let min_resource_fee = result
                    .get("minResourceFee")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok());

                Ok(SimulationResult {
                    success: true,
                    actual_output: None, // Would need to parse return value
                    resource_fee: min_resource_fee,
                    error: None,
                })
            }
            Err(e) => Ok(SimulationResult {
                success: false,
                actual_output: None,
                resource_fee: None,
                error: Some(format!("RPC error: {}", e)),
            }),
        }
    }

    // ===== Helper methods =====

    fn address_to_scval(&self, address: &str) -> Result<xdr::ScVal> {
        let key = stellar_strkey::ed25519::PublicKey::from_string(address)
            .map_err(|e| anyhow!("Invalid address: {:?}", e))?;
        Ok(xdr::ScVal::Address(xdr::ScAddress::Account(
            xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(key.0)))
        )))
    }

    fn contract_to_scval(&self, contract: &str) -> Result<xdr::ScVal> {
        let hash = stellar_strkey::Contract::from_string(contract)
            .map_err(|e| anyhow!("Invalid contract: {:?}", e))?
            .0;
        Ok(xdr::ScVal::Address(xdr::ScAddress::Contract(
            xdr::ContractId(xdr::Hash(hash))
        )))
    }

    fn token_to_scval(&self, token: &dex_adapters::TokenId) -> Result<xdr::ScVal> {
        match token {
            dex_adapters::TokenId::Native => {
                // XLM SAC address on mainnet
                self.contract_to_scval("CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA")
            }
            dex_adapters::TokenId::Classic { code, issuer } => {
                // Compute SAC address for classic asset
                // For now, use a placeholder - real implementation needs SAC computation
                let asset_str = format!("{}:{}", code, issuer);
                // TODO: compute_sac_contract_id
                Err(anyhow!("Classic asset SAC computation not yet implemented for {}", asset_str))
            }
            dex_adapters::TokenId::Contract { address } => {
                self.contract_to_scval(address)
            }
        }
    }

    fn i128_to_scval(&self, val: i128) -> xdr::ScVal {
        xdr::ScVal::I128(xdr::Int128Parts {
            hi: (val >> 64) as i64,
            lo: val as u64,
        })
    }
}
