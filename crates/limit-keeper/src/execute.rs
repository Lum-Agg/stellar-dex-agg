use {
    crate::{
        book::{OpenOrder, OrderKind},
        limit::required_min_out,
        quote::{steps_from_api_sub_route, Quote, QuoteStep, QuoteSubRoute},
    },
    anyhow::{anyhow, Context, Result},
    soroban_client::{
        keypair::{Keypair, KeypairBehavior},
        soroban_rpc::{SendTransactionStatus, TransactionStatus},
        transaction::{assemble_transaction, AccountBehavior, Transaction, TransactionBehavior},
        transaction_builder::{TransactionBuilder, TransactionBuilderBehavior, TIMEOUT_INFINITE},
        Options, Server,
    },
    std::time::Duration,
    stellar_strkey::Contract,
    stellar_xdr::curr::{self as xdr, Limits, ReadXdr, WriteXdr},
};

const BASE_FEE: u32 = 100_000;

pub fn fill_amount(order: &OpenOrder, max_fill: Option<i128>) -> i128 {
    let requested = order.chunk_amount.unwrap_or(order.amount_in_remaining);
    max_fill
        .filter(|cap| *cap > 0)
        .map_or(requested, |cap| requested.min(cap))
        .min(order.amount_in_remaining)
}

pub fn fill_min_amount_out(order: &OpenOrder, amount_in: i128, quote: &Quote) -> i128 {
    if order.limit_out_per_in_e7 > 0 {
        required_min_out(amount_in, order.limit_out_per_in_e7).max(quote.minimum_output)
    } else {
        quote.minimum_output
    }
}

/// Simulate, sign, submit, and wait briefly for an escrow `fill`.
///
/// This function never implements a dry-run bypass: callers must not invoke it
/// when dry-run is set, making the no-submit invariant explicit in the loop.
pub async fn execute_fill(
    rpc_url: &str,
    network_passphrase: &str,
    secret: &str,
    escrow_contract: &str,
    order: &OpenOrder,
    amount_in: i128,
    quote: &Quote,
) -> Result<String> {
    if amount_in <= 0 || amount_in > order.amount_in_remaining {
        return Err(anyhow!("invalid fill amount {amount_in} for order {}", order.order_id));
    }
    let operation = build_fill_operation(
        escrow_contract,
        order.kind,
        order.order_id,
        amount_in,
        &quote.sub_routes,
        fill_min_amount_out(order, amount_in, quote),
    )?;
    let keypair = Keypair::from_secret(secret).map_err(|e| anyhow!("invalid KEEPER_SECRET: {e:?}"))?;
    let public_key = keypair.public_key();
    let sequence = fetch_account_sequence(rpc_url, &public_key).await?;
    let unsigned_xdr =
        simulate_and_assemble(rpc_url, network_passphrase, &public_key, sequence as u64 + 1, operation).await?;

    let mut tx = transaction_from_prepared_xdr(&unsigned_xdr, network_passphrase)?;
    tx.sign(&[keypair]);
    let server = rpc_server(rpc_url)?;
    let submitted = server
        .send_transaction(tx)
        .await
        .map_err(|e| anyhow!("send fill transaction: {e:?}"))?;
    if submitted.status == SendTransactionStatus::Error {
        return Err(anyhow!("fill transaction rejected: {:?}", submitted.to_error_result()));
    }
    poll_transaction(rpc_url, &submitted.hash).await?;
    Ok(submitted.hash)
}

pub fn build_reclaim_operation(escrow_contract: &str, order_id: u64) -> Result<xdr::Operation> {
    contract_operation(escrow_contract, "reclaim_expired", vec![xdr::ScVal::U64(order_id)])
}

fn build_fill_operation(
    escrow_contract: &str,
    kind: OrderKind,
    order_id: u64,
    amount_in: i128,
    sub_routes: &[QuoteSubRoute],
    min_amount_out: i128,
) -> Result<xdr::Operation> {
    if sub_routes.is_empty() {
        return Err(anyhow!("quote has no sub_routes"));
    }
    let routes: Vec<xdr::ScVal> = sub_routes.iter().map(quote_sub_route_scval).collect::<Result<_>>()?;
    let routed: i128 = sub_routes
        .iter()
        .map(|route| route.amount_in.parse::<i128>().context("parse sub-route amount_in"))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .sum();
    if routed != amount_in {
        return Err(anyhow!(
            "quote route amount {routed} does not equal fill amount {amount_in}"
        ));
    }
    let routes = xdr::ScVal::Vec(Some(routes.try_into().map_err(|_| anyhow!("too many sub-routes"))?));
    match kind {
        OrderKind::Limit => contract_operation(
            escrow_contract,
            "fill",
            vec![
                xdr::ScVal::U64(order_id),
                i128_scval(amount_in),
                routes,
                i128_scval(min_amount_out),
            ],
        ),
        OrderKind::Dca => contract_operation(
            escrow_contract,
            "fill_dca",
            vec![xdr::ScVal::U64(order_id), routes, i128_scval(min_amount_out)],
        ),
    }
}

fn quote_sub_route_scval(route: &QuoteSubRoute) -> Result<xdr::ScVal> {
    let amount_in = route.amount_in.parse::<i128>().context("parse sub-route amount_in")?;
    let route_steps = steps_from_api_sub_route(route)?;
    if amount_in <= 0 || route_steps.is_empty() {
        return Err(anyhow!("sub-route must have a positive input and at least one step"));
    }
    let steps = route_steps.iter().map(quote_step_scval).collect::<Result<Vec<_>>>()?;
    map_scval(vec![
        ("amount_in", i128_scval(amount_in)),
        (
            "steps",
            xdr::ScVal::Vec(Some(steps.try_into().map_err(|_| anyhow!("too many route steps"))?)),
        ),
    ])
}

fn quote_step_scval(step: &QuoteStep) -> Result<xdr::ScVal> {
    let dex_type = match step.dex_type.as_str() {
        "aquarius" | "aquarius_clmm" => "Aquarius",
        "soroswap" => "SoroswapPair",
        "phoenix" => "Phoenix",
        "sushi" => "Sushi",
        "comet" => "CometDex",
        other => return Err(anyhow!("unsupported keeper DEX route type: {other}")),
    };
    map_scval(vec![
        ("dex_id", contract_scval(&step.pool_address)?),
        (
            "dex_type",
            xdr::ScVal::Vec(Some(
                vec![xdr::ScVal::Symbol(xdr::ScSymbol(dex_type.try_into().unwrap()))]
                    .try_into()
                    .map_err(|_| anyhow!("DEX enum"))?,
            )),
        ),
        ("in_idx", xdr::ScVal::U32(step.in_idx)),
        ("out_idx", xdr::ScVal::U32(step.out_idx)),
        ("token_in", contract_scval(&step.token_in)?),
        ("token_out", contract_scval(&step.token_out)?),
    ])
}

fn map_scval(entries: Vec<(&str, xdr::ScVal)>) -> Result<xdr::ScVal> {
    let values = entries
        .into_iter()
        .map(|(key, val)| xdr::ScMapEntry {
            key: xdr::ScVal::Symbol(xdr::ScSymbol(key.try_into().unwrap())),
            val,
        })
        .collect::<Vec<_>>();
    Ok(xdr::ScVal::Map(Some(xdr::ScMap(
        values.try_into().map_err(|_| anyhow!("ScVal map too large"))?,
    ))))
}

fn contract_scval(value: &str) -> Result<xdr::ScVal> {
    let hash = Contract::from_string(value)
        .with_context(|| format!("invalid contract ID {value}"))?
        .0;
    Ok(xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(
        xdr::Hash(hash),
    ))))
}

fn i128_scval(value: i128) -> xdr::ScVal {
    xdr::ScVal::I128(xdr::Int128Parts {
        hi: (value >> 64) as i64,
        lo: value as u64,
    })
}

fn contract_operation(contract: &str, function: &str, args: Vec<xdr::ScVal>) -> Result<xdr::Operation> {
    let hash = Contract::from_string(contract)
        .with_context(|| format!("invalid escrow contract ID {contract}"))?
        .0;
    Ok(xdr::Operation {
        source_account: None,
        body: xdr::OperationBody::InvokeHostFunction(xdr::InvokeHostFunctionOp {
            host_function: xdr::HostFunction::InvokeContract(xdr::InvokeContractArgs {
                contract_address: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(hash))),
                function_name: xdr::ScSymbol(function.try_into().map_err(|_| anyhow!("invalid function name"))?),
                args: args.try_into().map_err(|_| anyhow!("too many invocation args"))?,
            }),
            auth: xdr::VecM::default(),
        }),
    })
}

fn rpc_server(rpc_url: &str) -> Result<Server> {
    Server::new(
        rpc_url,
        Options {
            allow_http: true,
            ..Default::default()
        },
    )
    .map_err(|e| anyhow!("create Soroban RPC client: {e}"))
}

async fn fetch_account_sequence(rpc_url: &str, public_key: &str) -> Result<i64> {
    use serde_json::json;
    let key = stellar_strkey::ed25519::PublicKey::from_string(public_key)
        .map_err(|e| anyhow!("invalid keeper public key: {e:?}"))?;
    let ledger_key = xdr::LedgerKey::Account(xdr::LedgerKeyAccount {
        account_id: xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(key.0))),
    });
    let encoded = ledger_key
        .to_xdr_base64(Limits::none())
        .context("encode keeper ledger key")?;
    let response: serde_json::Value = reqwest::Client::new()
        .post(rpc_url)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"getLedgerEntries","params":{"keys":[encoded]}}))
        .send()
        .await
        .context("get keeper account")?
        .json()
        .await
        .context("decode keeper account")?;
    let entry = response
        .pointer("/result/entries/0/xdr")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("keeper account is not present on the ledger"))?;
    let entry = xdr::LedgerEntry::from_xdr_base64(entry, Limits::none()).context("decode keeper account entry")?;
    match entry.data {
        xdr::LedgerEntryData::Account(account) => Ok(account.seq_num.0),
        _ => Err(anyhow!("keeper ledger entry is not an account")),
    }
}

async fn simulate_and_assemble(
    rpc_url: &str,
    network_passphrase: &str,
    public_key: &str,
    sequence: u64,
    operation: xdr::Operation,
) -> Result<String> {
    let mut account = soroban_client::account::Account::new(public_key, &sequence.to_string())
        .map_err(|e| anyhow!("create keeper account: {e}"))?;
    let bytes = operation.to_xdr(Limits::none()).context("encode fill operation")?;
    let operation = {
        use soroban_client::xdr::{Limits as ClientLimits, ReadXdr};
        soroban_client::xdr::Operation::from_xdr(bytes, ClientLimits::none()).context("decode fill operation")?
    };
    let mut builder = TransactionBuilder::new(&mut account, network_passphrase, None);
    builder.fee(BASE_FEE);
    builder.add_operation(operation);
    let tx = builder
        .set_timeout(TIMEOUT_INFINITE)
        .map_err(|e| anyhow!("set fill timeout: {e}"))?
        .build();
    let simulation = rpc_server(rpc_url)?
        .simulate_transaction(&tx, None)
        .await
        .map_err(|e| anyhow!("simulate fill transaction: {e:?}"))?;
    if simulation.error.is_some() {
        return Err(anyhow!(
            "fill simulation failed: {}",
            simulation.error.unwrap_or_default()
        ));
    }
    let assembled = assemble_transaction(&tx, simulation).map_err(|e| anyhow!("assemble fill transaction: {e:?}"))?;
    {
        use soroban_client::xdr::{Limits as ClientLimits, WriteXdr};
        assembled
            .to_envelope()
            .map_err(|e| anyhow!("fill envelope: {e}"))?
            .to_xdr_base64(ClientLimits::none())
            .context("encode assembled fill transaction")
    }
}

fn transaction_from_prepared_xdr(unsigned_xdr: &str, network_passphrase: &str) -> Result<Transaction> {
    use stellar_baselib::xdr::{Limits as BaseLimits, ReadXdr, TransactionEnvelope, TransactionExt};
    let mut tx = Transaction::from_xdr_envelope(unsigned_xdr, network_passphrase);
    let envelope = TransactionEnvelope::from_xdr_base64(unsigned_xdr, BaseLimits::none())
        .context("parse assembled transaction")?;
    if let TransactionEnvelope::Tx(v1) = envelope {
        if let TransactionExt::V1(data) = v1.tx.ext {
            tx.soroban_data = Some(data);
        }
    }
    Ok(tx)
}

async fn poll_transaction(rpc_url: &str, hash: &str) -> Result<()> {
    let server = rpc_server(rpc_url)?;
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let status = server
            .get_transaction(hash)
            .await
            .map_err(|e| anyhow!("poll fill {hash}: {e:?}"))?;
        if status.status == TransactionStatus::Success {
            return Ok(());
        }
        if status.status == TransactionStatus::Failed {
            return Err(anyhow!("fill {hash} failed on-chain: {:?}", status.to_result()));
        }
    }
    Err(anyhow!("fill {hash} was not confirmed before poll timeout"))
}

#[cfg(test)]
mod tests {
    use crate::{
        book::{OpenOrder, OrderKind},
        execute::{fill_amount, fill_min_amount_out},
        quote::Quote,
    };

    fn order() -> OpenOrder {
        OpenOrder {
            kind: OrderKind::Limit,
            order_id: 7,
            owner: "owner".into(),
            token_in: "in".into(),
            token_out: "out".into(),
            amount_in_remaining: 500,
            limit_out_per_in_e7: 20_000_000,
            expires_ledger: 999,
            chunk_amount: None,
            next_executable_ledger: None,
        }
    }

    #[test]
    fn caps_fill_and_uses_stricter_quote_floor() {
        let quote = Quote {
            expected_output: 800,
            minimum_output: 650,
            sub_routes: vec![],
        };

        assert_eq!(fill_amount(&order(), Some(300)), 300);
        assert_eq!(fill_min_amount_out(&order(), 300, &quote), 650);
    }
}
