//! Independent quote-api vs on-chain hop probe (does not run inside
//! arb-scanner).
//!
//! Usage:
//!   ARB_QUOTE_API_URLS=http://127.0.0.1:3100 RPC_URL=http://127.0.0.1:8003 \
//!     cargo run -p arbitrage --bin quote-sim-probe -- \
//!     --mode one-leg --token-in CAS3...OWMA --token-out CCW67...MI75
//! --amount-in 100000000

use {
    anyhow::{bail, Context, Result},
    arbitrage::{
        bridge::quote_round_trip,
        config::ArbConfig,
        context::ArbContext,
        invoke::{build_execute_round_trip_op, min_amount_out_break_even},
        prepare::{
            fetch_account_sequence, fetch_latest_ledger, parse_base_received_from_sim_error, prepare_transaction_xdr,
            vault_allowance_expiration,
        },
        probe::{
            first_diverging_hop, hop_gap_bps, output_gap_bps, pick_bridges, round_trip_abs_gap_bps, HopCompare,
            HopCompareReport, ProbeSampleReport, RoundTripProbeReport,
        },
        scanner::compute_profit_bps,
    },
    dex_adapters::{on_chain_quote, rpc::SorobanRpc},
    router_engine::TokenId,
    serde_json::Value,
    std::env,
};

const DEFAULT_QUOTE_API: &str = "http://127.0.0.1:3100";
const DEFAULT_RPC_URL: &str = "https://soroban-rpc.mainnet.stellar.gateway.fm";
const STELLAR_MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";

struct Cli {
    mode: String,
    token_in: Option<String>,
    token_out: Option<String>,
    amount_in: u128,
    samples: usize,
    seed: u64,
    threshold_bps: i64,
    jsonl: bool,
    simulate: bool,
    bridges: Option<Vec<String>>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            mode: "one-leg".into(),
            token_in: None,
            token_out: None,
            amount_in: 100_000_000,
            samples: 1,
            seed: 1,
            threshold_bps: 10,
            jsonl: false,
            simulate: false,
            bridges: None,
        }
    }
}

fn error_report(token_in: &str, token_out: &str, amount_in: u128, error: impl Into<String>) -> ProbeSampleReport {
    ProbeSampleReport {
        mode: "one-leg".into(),
        token_in: token_in.into(),
        token_out: token_out.into(),
        amount_in,
        local_out: 0,
        chain_path_out: None,
        gap_bps: None,
        first_bad_hop: None,
        hops: vec![],
        simulate_out: None,
        simulate_gap_bps: None,
        error: Some(error.into()),
    }
}

async fn compare_quote_leg(
    quote_api: &str,
    rpc: &SorobanRpc,
    token_in: &str,
    token_out: &str,
    amount_in: u128,
    threshold_bps: i64,
) -> Result<ProbeSampleReport> {
    let url = format!(
        "{quote_api}/api/v1/quote?token_in={token_in}&token_out={token_out}&amount_in={amount_in}&prefer_soroban=1&max_splits=1"
    );
    let body: Value = match reqwest::get(&url).await {
        Ok(response) => match response.json().await {
            Ok(body) => body,
            Err(error) => {
                return Ok(error_report(
                    token_in,
                    token_out,
                    amount_in,
                    format!("quote response JSON failed: {error}"),
                ));
            }
        },
        Err(error) => {
            return Ok(error_report(
                token_in,
                token_out,
                amount_in,
                format!("quote request failed: {error}"),
            ));
        }
    };

    if body["success"].as_bool() != Some(true) {
        return Ok(error_report(
            token_in,
            token_out,
            amount_in,
            format!("quote failed: {body}"),
        ));
    }

    let data = &body["data"];
    let local_out = match data["expected_output"].as_str().unwrap_or("0").parse() {
        Ok(value) => value,
        Err(error) => {
            return Ok(error_report(
                token_in,
                token_out,
                amount_in,
                format!("invalid quote expected_output: {error}"),
            ));
        }
    };
    let Some(sub) = data["sub_routes"].as_array().and_then(|routes| routes.first()) else {
        return Ok(error_report(
            token_in,
            token_out,
            amount_in,
            "quote returned no sub_routes",
        ));
    };

    let sources: Vec<String> = sub["dex_types"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();
    let pools: Vec<String> = sub["pool_addresses"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();
    let tokens: Vec<String> = sub["path"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();
    let in_indices: Vec<u32> = sub["in_indices"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|value| value.as_u64().and_then(|number| number.try_into().ok()))
        .collect();
    let out_indices: Vec<u32> = sub["out_indices"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|value| value.as_u64().and_then(|number| number.try_into().ok()))
        .collect();

    if sources.is_empty() ||
        pools.len() != sources.len() ||
        tokens.len() != sources.len() + 1 ||
        in_indices.len() != sources.len() ||
        out_indices.len() != sources.len()
    {
        return Ok(error_report(
            token_in,
            token_out,
            amount_in,
            "quote returned an incomplete first sub_route",
        ));
    }

    let mut hops = Vec::with_capacity(sources.len());
    let mut current = amount_in;
    let mut chain_path_out = Some(amount_in);
    for index in 0..sources.len() {
        let chain_out = match on_chain_quote::hop_amount_out_on_chain(
            rpc,
            &sources[index],
            &pools[index],
            &tokens[index],
            &tokens[index + 1],
            in_indices[index],
            out_indices[index],
            current,
        )
        .await
        {
            Ok(value) => value,
            Err(error) => {
                return Ok(error_report(
                    token_in,
                    token_out,
                    amount_in,
                    format!("on-chain hop {index} failed: {error}"),
                ));
            }
        };

        hops.push(HopCompare {
            index,
            source: sources[index].clone(),
            pool: pools[index].clone(),
            amount_in: current,
            // Public quote-api omits per-hop amounts; only the final hop can
            // carry the path-level local output.
            local_out: 0,
            chain_out,
        });
        match chain_out {
            Some(value) if value > 0 => current = value,
            _ => {
                chain_path_out = None;
                break;
            }
        }
    }

    if chain_path_out.is_some() {
        chain_path_out = Some(current);
        if let Some(last_hop) = hops.last_mut() {
            last_hop.local_out = local_out;
        }
    }

    // One-leg outs are different tokens — use output-relative gap, not RT profit
    // bps.
    let gap_bps = chain_path_out.map(|chain_out| output_gap_bps(local_out, chain_out));
    let first_bad_hop = first_diverging_hop(&hops, threshold_bps);

    Ok(ProbeSampleReport {
        mode: "one-leg".into(),
        token_in: token_in.into(),
        token_out: token_out.into(),
        amount_in,
        local_out,
        chain_path_out,
        gap_bps,
        first_bad_hop,
        hops: hops.iter().map(HopCompareReport::from).collect(),
        simulate_out: None,
        simulate_gap_bps: None,
        error: None,
    })
}

fn round_trip_error_report(
    base: &str,
    bridge: &str,
    amount_in: u128,
    error: impl Into<String>,
) -> RoundTripProbeReport {
    RoundTripProbeReport {
        bridge: bridge.into(),
        amount_in,
        quoted_out: 0,
        quoted_profit_bps: 0,
        leg_out: error_report(base, bridge, amount_in, "round-trip quote unavailable"),
        leg_back: error_report(bridge, base, 0, "round-trip quote unavailable"),
        simulate_out: None,
        simulate_gap_bps: None,
        error: Some(error.into()),
    }
}

async fn simulate_round_trip(
    ctx: &ArbContext,
    base: &TokenId,
    bridge: &TokenId,
    caller: &str,
    amount_in: u128,
    quote: &arbitrage::bridge::RoundTripQuote,
) -> Result<u128> {
    let vault = ctx
        .config
        .vault_contract
        .as_deref()
        .context("ARB_VAULT_CONTRACT is required for --simulate")?;
    let aggregator = ctx
        .config
        .aggregator_contract
        .as_deref()
        .context("ARB_AGGREGATOR_CONTRACT is required for --simulate")?;
    // Match arb-scanner: break-even min_out so sim returns the real on-chain
    // amount instead of trapping when quote.minimum_out is optimistic.
    let min_out = min_amount_out_break_even(amount_in);
    let allowance_expiration = vault_allowance_expiration(fetch_latest_ledger(&ctx.config.rpc_url).await?);
    let op = build_execute_round_trip_op(
        vault,
        aggregator,
        caller,
        &base.canonical(),
        &bridge.canonical(),
        i128::try_from(amount_in)?,
        &quote.leg_out,
        &quote.leg_back,
        min_out,
        allowance_expiration,
    )?;
    let sequence = u64::try_from(fetch_account_sequence(&ctx.config.rpc_url, caller).await?)?;
    match prepare_transaction_xdr(
        &ctx.config.rpc_url,
        caller,
        sequence,
        std::slice::from_ref(&op),
        100_000,
    )
    .await
    {
        Ok(prepared) => Ok(prepared.amount_out),
        Err(error) => {
            let err_str = error.to_string();
            if let Some(recovered) = parse_base_received_from_sim_error(&err_str, &base.canonical(), aggregator, caller)
            {
                return Ok(recovered);
            }
            Err(error)
        }
    }
}

async fn run_round_trip_sample(
    ctx: &ArbContext,
    rpc: &SorobanRpc,
    quote_api: &str,
    base: &TokenId,
    bridge: &str,
    amount_in: u128,
    threshold_bps: i64,
    simulate: bool,
) -> RoundTripProbeReport {
    let bridge_id = TokenId::from_str_auto(bridge);
    let base_name = base.canonical();
    let bridge_name = bridge_id.canonical();
    let quote = match quote_round_trip(ctx, base, &bridge_id, amount_in).await {
        Ok(quote) => quote,
        Err(error) => return round_trip_error_report(&base_name, &bridge_name, amount_in, error.to_string()),
    };

    // The quote may use split routes. These path-localization probes intentionally
    // request max_splits=1, so their per-leg route can differ from the execution
    // quote.
    let leg_out =
        match compare_quote_leg(quote_api, rpc, &base_name, &bridge_name, quote.amount_in, threshold_bps).await {
            Ok(report) => report,
            Err(error) => error_report(&base_name, &bridge_name, quote.amount_in, error.to_string()),
        };
    let leg_back = match compare_quote_leg(
        quote_api,
        rpc,
        &bridge_name,
        &base_name,
        quote.leg_out.route.total_expected_out,
        threshold_bps,
    )
    .await
    {
        Ok(report) => report,
        Err(error) => error_report(
            &bridge_name,
            &base_name,
            quote.leg_out.route.total_expected_out,
            error.to_string(),
        ),
    };

    let mut report = RoundTripProbeReport {
        bridge: bridge_name,
        amount_in: quote.amount_in,
        quoted_out: quote.amount_out,
        quoted_profit_bps: compute_profit_bps(quote.amount_in, quote.amount_out),
        leg_out,
        leg_back,
        simulate_out: None,
        simulate_gap_bps: None,
        error: None,
    };

    if simulate {
        let caller = env::var("ARB_PROBE_CALLER")
            .unwrap_or_else(|_| "GCMDWFAHD6PYI5SI2N2M6XINZDITECUV4XN7LYQGOWKQSIMQPRNK2DLN".into());
        match simulate_round_trip(ctx, base, &bridge_id, &caller, amount_in, &quote).await {
            Ok(simulate_out) => {
                report.simulate_gap_bps = Some(hop_gap_bps(quote.amount_in, quote.amount_out, simulate_out));
                report.simulate_out = Some(simulate_out);
            }
            Err(error) => report.error = Some(format!("simulate failed: {error:#}")),
        }
    }

    report
}

fn parse_args() -> Result<Cli> {
    let mut cli = Cli::default();
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--mode" => cli.mode = next_value(&mut args, "--mode")?,
            "--token-in" => cli.token_in = Some(next_value(&mut args, "--token-in")?),
            "--token-out" => cli.token_out = Some(next_value(&mut args, "--token-out")?),
            "--amount-in" => {
                cli.amount_in = next_value(&mut args, "--amount-in")?
                    .parse()
                    .context("--amount-in must be a u128")?
            }
            "--samples" => {
                cli.samples = next_value(&mut args, "--samples")?
                    .parse()
                    .context("--samples must be a usize")?
            }
            "--seed" => {
                cli.seed = next_value(&mut args, "--seed")?
                    .parse()
                    .context("--seed must be a u64")?
            }
            "--threshold-bps" => {
                cli.threshold_bps = next_value(&mut args, "--threshold-bps")?
                    .parse()
                    .context("--threshold-bps must be an i64")?
            }
            "--bridges" => {
                let bridges = next_value(&mut args, "--bridges")?
                    .split(',')
                    .map(str::trim)
                    .filter(|bridge| !bridge.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if bridges.is_empty() {
                    bail!("--bridges must contain at least one token");
                }
                cli.bridges = Some(bridges);
            }
            "--jsonl" => cli.jsonl = true,
            "--simulate" => cli.simulate = true,
            "--help" | "-h" => {
                bail!(
                    "usage: quote-sim-probe --mode one-leg --token-in C... --token-out C... \
                     --amount-in 100000000 [--samples N] [--seed U64] [--threshold-bps 10] \
                     [--jsonl] [--simulate]\n\
                     quote-sim-probe --mode round-trip [--bridges C...,C...] \
                     [--amount-in 100000000] [--samples N] [--seed U64] [--threshold-bps 10] \
                     [--jsonl] [--simulate]"
                )
            }
            _ => bail!("unknown argument: {argument}"),
        }
    }
    Ok(cli)
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next().with_context(|| format!("{flag} requires a value"))
}

fn quote_api_url() -> String {
    env::var("ARB_QUOTE_API_URLS")
        .ok()
        .and_then(|urls| {
            urls.split(',')
                .map(str::trim)
                .find(|url| !url.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| env::var("QUOTE_API_URL").ok())
        .unwrap_or_else(|| DEFAULT_QUOTE_API.into())
}

fn print_human(report: &ProbeSampleReport) {
    println!(
        "one-leg {} -> {} amount_in={} local_out={} chain_path_out={:?} gap_bps={:?} first_bad_hop={:?}",
        report.token_in,
        report.token_out,
        report.amount_in,
        report.local_out,
        report.chain_path_out,
        report.gap_bps,
        report.first_bad_hop
    );
    if let Some(error) = &report.error {
        println!("error: {error}");
    }
    for hop in &report.hops {
        println!(
            "  hop[{}] source={} pool={} in={} local_out={} chain_out={:?} gap_bps={:?}",
            hop.index, hop.source, hop.pool, hop.amount_in, hop.local_out, hop.chain_out, hop.gap_bps
        );
    }
}

fn print_round_trip_human(report: &RoundTripProbeReport) {
    println!(
        "round-trip bridge={} amount_in={} quoted_out={} quoted_profit_bps={} simulate_out={:?} simulate_gap_bps={:?}",
        report.bridge,
        report.amount_in,
        report.quoted_out,
        report.quoted_profit_bps,
        report.simulate_out,
        report.simulate_gap_bps,
    );
    if let Some(error) = &report.error {
        println!("error: {error}");
    }
    println!("leg_out:");
    print_human(&report.leg_out);
    println!("leg_back:");
    print_human(&report.leg_back);
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = parse_args()?;
    if cli.threshold_bps < 0 {
        bail!("--threshold-bps must be non-negative");
    }
    if cli.mode == "round-trip" {
        // ArbConfig requires bridge tokens even though an explicit --bridges
        // list should be enough for this standalone probe.
        if let Some(bridges) = &cli.bridges {
            env::set_var("ARB_BRIDGE_TOKENS", bridges.join(","));
        }
        let config = ArbConfig::from_env()?;
        let ctx = ArbContext::connect(config).await?;
        let base = ctx
            .config
            .base_tokens
            .first()
            .context("no base token configured")?
            .clone();
        let bridges = cli
            .bridges
            .clone()
            .unwrap_or_else(|| ctx.config.bridge_tokens.iter().map(TokenId::canonical).collect());
        if bridges.is_empty() {
            bail!("no bridge tokens configured; set ARB_BRIDGE_TOKENS or pass --bridges");
        }

        let rpc = SorobanRpc::new(&ctx.config.rpc_url, STELLAR_MAINNET_PASSPHRASE);
        let quote_api = ctx
            .config
            .quote_api_urls
            .first()
            .cloned()
            .context("no quote API URL configured")?;
        let mut abs_gaps = Vec::new();
        for bridge in pick_bridges(&bridges, cli.samples, cli.seed) {
            let report = run_round_trip_sample(
                &ctx,
                &rpc,
                &quote_api,
                &base,
                &bridge,
                cli.amount_in,
                cli.threshold_bps,
                cli.simulate,
            )
            .await;
            if let Some(gap) = round_trip_abs_gap_bps(&report) {
                abs_gaps.push(gap);
            }
            if cli.jsonl {
                println!("{}", serde_json::to_string(&report)?);
            } else {
                print_round_trip_human(&report);
            }
        }
        abs_gaps.sort_unstable();
        if cli.samples > 0 && abs_gaps.is_empty() {
            eprintln!(
                "quote-sim-probe: no measurable gaps across {} samples (all failed or incomplete); exiting 1",
                cli.samples
            );
            std::process::exit(1);
        }
        if let Some(median) = abs_gaps.get(abs_gaps.len() / 2) {
            if *median > cli.threshold_bps as u64 {
                std::process::exit(1);
            }
        }
        return Ok(());
    } else if cli.mode != "one-leg" {
        bail!("--mode must be one-leg or round-trip");
    }
    let token_in = cli.token_in.context("--token-in is required for one-leg mode")?;
    let token_out = cli.token_out.context("--token-out is required for one-leg mode")?;

    let _ = (cli.samples, cli.seed, cli.simulate);
    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| DEFAULT_RPC_URL.into());
    let rpc = SorobanRpc::new(&rpc_url, STELLAR_MAINNET_PASSPHRASE);
    let report = compare_quote_leg(
        &quote_api_url(),
        &rpc,
        &token_in,
        &token_out,
        cli.amount_in,
        cli.threshold_bps,
    )
    .await?;

    if cli.jsonl {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        print_human(&report);
    }
    Ok(())
}
