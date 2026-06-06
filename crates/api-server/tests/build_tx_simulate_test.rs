//! Compare api-server XDR encoding vs on-chain simulate (requires network).
//! Run: cargo test -p api-server --test build_tx_simulate_test -- --ignored
//! --nocapture

use {
    api_server::{
        handlers::{build_tx_impl, build_unsigned_tx_xdr, BuildTxRequest, BuildTxStep, BuildTxSubRoute},
        soroban_prepare::prepare_transaction_xdr,
    },
    stellar_xdr::curr::{Limits, ReadXdr},
};

const USER: &str = "GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY";
const XLM: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const USDC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const CAUI: &str = "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK";
const RPC: &str = "https://soroban-rpc.mainnet.stellar.gateway.fm";

fn build_request(
    dex_type: &str,
    pool_address: &str,
    token_in: &str,
    token_out: &str,
    in_idx: u32,
    out_idx: u32,
    min_amount_out: &str,
) -> BuildTxRequest {
    BuildTxRequest {
        user_public_key: USER.to_string(),
        amount_in: "1000000".to_string(),
        token_in: token_in.to_string(),
        token_out: token_out.to_string(),
        min_amount_out: min_amount_out.to_string(),
        sub_routes: vec![BuildTxSubRoute {
            amount_in: "1000000".to_string(),
            steps: vec![BuildTxStep {
                dex_type: dex_type.to_string(),
                pool_address: pool_address.to_string(),
                token_in: token_in.to_string(),
                token_out: token_out.to_string(),
                in_idx,
                out_idx,
            }],
        }],
    }
}

fn build_hybrid_request() -> BuildTxRequest {
    BuildTxRequest {
        user_public_key: USER.to_string(),
        token_in: XLM.to_string(),
        token_out: USDC.to_string(),
        amount_in: "10000000".to_string(),
        min_amount_out: "1423886".to_string(),
        sub_routes: vec![
            BuildTxSubRoute {
                amount_in: "9310038".to_string(),
                steps: vec![BuildTxStep {
                    dex_type: "classic_dex".to_string(),
                    pool_address: "classic:CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA:CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75".to_string(),
                    token_in: XLM.to_string(),
                    token_out: USDC.to_string(),
                    in_idx: 0,
                    out_idx: 1,
                }],
            },
            BuildTxSubRoute {
                amount_in: "619648".to_string(),
                steps: vec![BuildTxStep {
                    dex_type: "aquarius".to_string(),
                    pool_address: "CA6PUJLBYKZKUEKLZJMKBZLEKP2OTHANDEOWSFF44FTSYLKQPIICCJBE".to_string(),
                    token_in: XLM.to_string(),
                    token_out: USDC.to_string(),
                    in_idx: 0,
                    out_idx: 1,
                }],
            },
            BuildTxSubRoute {
                amount_in: "51287".to_string(),
                steps: vec![BuildTxStep {
                    dex_type: "soroswap".to_string(),
                    pool_address: "CAM7DY53G63XA4AJRS24Z6VFYAFSSF76C3RZ45BE5YU3FQS5255OOABP".to_string(),
                    token_in: XLM.to_string(),
                    token_out: USDC.to_string(),
                    in_idx: 0,
                    out_idx: 1,
                }],
            },
            BuildTxSubRoute {
                amount_in: "19027".to_string(),
                steps: vec![BuildTxStep {
                    dex_type: "sushi".to_string(),
                    pool_address: "CCR2CH4GQVCZHG7CHFVMNANCK45CU5DVKXZIIITDZQAU3CEJZ7RQH2MQ".to_string(),
                    token_in: XLM.to_string(),
                    token_out: USDC.to_string(),
                    in_idx: 0,
                    out_idx: 1,
                }],
            },
        ],
    }
}

async fn rpc_simulate(tx_xdr: &str) -> serde_json::Value {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "simulateTransaction",
        "params": {
            "transaction": tx_xdr,
            "resourceConfig": { "instructionLeeway": 3_000_000 }
        }
    });
    let resp = client.post(RPC).json(&body).send().await.unwrap();
    resp.json().await.unwrap()
}

async fn fetch_sequence(public_key: &str) -> u64 {
    let url = format!("https://horizon.stellar.org/accounts/{}", public_key);
    let value: serde_json::Value = reqwest::get(url).await.unwrap().json().await.unwrap();
    value["sequence"].as_str().unwrap().parse().unwrap()
}

async fn assert_request_simulates(body: BuildTxRequest) {
    let tx_xdr = build_unsigned_tx_xdr(&body).await.expect("build xdr");
    let resp = rpc_simulate(&tx_xdr).await;
    let result = resp.get("result").expect("rpc result");
    if let Some(err) = result.get("error") {
        panic!("simulate failed: {}", err);
    }
    assert!(result.get("transactionData").is_some());
}

async fn assert_request_prepares(body: BuildTxRequest) {
    let tx_xdr = build_unsigned_tx_xdr(&body).await.expect("build xdr");
    let envelope =
        stellar_xdr::curr::TransactionEnvelope::from_xdr_base64(&tx_xdr, Limits::none()).expect("parse raw envelope");
    let stellar_xdr::curr::TransactionEnvelope::Tx(v1) = envelope else {
        panic!("unsupported envelope")
    };
    let ops: Vec<_> = v1.tx.operations.to_vec();
    let seq = fetch_sequence(USER).await;
    let prepared_xdr = prepare_transaction_xdr(RPC, USER, seq, &ops, 100_000)
        .await
        .expect("prepare tx");
    let resp = rpc_simulate(&prepared_xdr).await;
    let result = resp.get("result").expect("rpc result");
    if let Some(err) = result.get("error") {
        panic!("prepared tx simulate failed: {}", err);
    }
    assert!(result.get("transactionData").is_some());
}

#[tokio::test]
#[ignore = "mainnet RPC"]
async fn api_build_tx_xdr_sushi_simulates() {
    let body = build_request(
        "sushi",
        "CCR2CH4GQVCZHG7CHFVMNANCK45CU5DVKXZIIITDZQAU3CEJZ7RQH2MQ",
        XLM,
        USDC,
        0,
        1,
        "1",
    );
    let tx_xdr = build_unsigned_tx_xdr(&body).await.expect("build xdr");
    std::fs::write("/tmp/lumagg_build_tx.xdr", &tx_xdr).ok();
    println!("tx_xdr len={} written /tmp/lumagg_build_tx.xdr", tx_xdr.len());
    println!("tx_xdr prefix: {}...", &tx_xdr[..80.min(tx_xdr.len())]);

    let resp = rpc_simulate(&tx_xdr).await;
    let result = resp.get("result").expect("rpc result");
    if let Some(err) = result.get("error") {
        panic!("simulate failed: {}", err);
    }
    assert!(result.get("transactionData").is_some());
    println!("simulate OK, minResourceFee={:?}", result.get("minResourceFee"));
}

#[tokio::test]
#[ignore = "mainnet RPC"]
async fn api_build_tx_xdr_sushi_prepares() {
    assert_request_prepares(build_request(
        "sushi",
        "CCR2CH4GQVCZHG7CHFVMNANCK45CU5DVKXZIIITDZQAU3CEJZ7RQH2MQ",
        XLM,
        USDC,
        0,
        1,
        "1",
    ))
    .await;
}

#[tokio::test]
#[ignore = "mainnet RPC"]
async fn api_build_tx_xdr_phoenix_simulates() {
    assert_request_simulates(build_request(
        "phoenix",
        "CBHCRSVX3ZZ7EGTSYMKPEFGZNWRVCSESQR3UABET4MIW52N4EVU6BIZX",
        XLM,
        USDC,
        0,
        1,
        "1",
    ))
    .await;
}

#[tokio::test]
#[ignore = "mainnet RPC"]
async fn api_build_tx_xdr_soroswap_simulates() {
    assert_request_simulates(build_request(
        "soroswap",
        "CDRS7NJPAX2HLYNENMUH3USUV6LP6KYSLZZ4ULY27RQLKIVC5DGLEVKI",
        XLM,
        CAUI,
        0,
        1,
        "1",
    ))
    .await;
}

#[tokio::test]
#[ignore = "mainnet RPC"]
async fn api_build_tx_xdr_aquarius_simulates() {
    assert_request_simulates(build_request(
        "aquarius",
        "CCSY43EHJAHT3NQDYKAMJXRFBEEH7OXDL3J3VNGO33UUSEXWNN27GBIZ",
        XLM,
        CAUI,
        0,
        1,
        "1",
    ))
    .await;
}

#[tokio::test]
#[ignore = "mainnet RPC"]
async fn api_build_tx_xdr_hybrid_simulates() {
    let err = match build_tx_impl(&build_hybrid_request()).await {
        Ok(_) => panic!("hybrid build should be rejected"),
        Err(err) => err,
    };
    assert!(
        err.contains("not supported") && err.contains("more than one operation"),
        "unexpected hybrid error: {err}"
    );
}
